use crate::{Ctx, UiEvent};
use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::Ordering};

pub type Slot = u64;

pub async fn store_memos(ctx: Arc<Ctx>, memos: Vec<(String, memos::Memo)>, slot: Option<Slot>) {
    let con = ctx.redis.lock().await;
    let mut con = match con.get_multiplexed_async_connection().await {
        Ok(con) => con,
        Err(e) => {
            tracing::error!("Redis connection error: {e}");
            return;
        }
    };

    let mut pipe = redis::pipe();
    pipe.atomic();
    for (pda, memo) in &memos {
        let save: MemoSave = memo.into();
        let ser_memo = serde_json::to_string(&save).unwrap();
        pipe.set(format!("memo-{pda}"), &ser_memo).ignore();
    }
    if let Some(ref key) = slot {
        pipe.set("slot", key).ignore();
    }

    if let Err(e) = pipe.query_async::<()>(&mut con).await {
        tracing::error!("Redis error: {e}");
    }

    let _ = ctx.tx.send(UiEvent::Stored(memos));
}

pub async fn load_slot(ctx: Arc<Ctx>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;
    if let Some(slot) = con.get("slot").await? {
        tracing::info!("Loaded slot from cache. Value: {slot}");
        ctx.slot.store(slot, Ordering::Relaxed);
    }

    Ok(())
}

pub async fn load_memos(ctx: Arc<Ctx>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;
    let keys: Vec<String> = con.keys("memo-*").await?;

    if keys.is_empty() {
        return Ok(());
    }

    let values: Vec<String> = con.mget(&keys).await?;

    let values: Vec<Option<memos::Memo>> = values
        .into_iter()
        .map(|m| {
            let save: Option<MemoSave> = serde_json::from_str(&m)
                .inspect_err(|e| tracing::error!("Error deserializing memo from redis: {e:?}"))
                .ok();
            save
        })
        .map(|m| m.map(Into::into))
        .collect();

    let memos = keys
        .into_iter()
        .map(|k| k.trim_start_matches("memo-").to_string())
        .zip(values)
        .filter_map(|(k, v)| Some((k, v?)))
        .collect();

    let _ = ctx.tx.send(UiEvent::Stored(memos));

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct MemoSave {
    memo: String,
    count: u64,
}

impl From<memos::Memo> for MemoSave {
    fn from(memo: memos::Memo) -> Self {
        Self {
            memo: memo.memo,
            count: memo.count,
        }
    }
}
impl From<&memos::Memo> for MemoSave {
    fn from(memo: &memos::Memo) -> Self {
        memo.clone().into()
    }
}

impl From<MemoSave> for memos::Memo {
    fn from(save: MemoSave) -> Self {
        Self {
            memo: save.memo,
            count: save.count,
        }
    }
}
