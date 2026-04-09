use crate::{Ctx, UiEvent};
use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type Slot = u64;

pub async fn store_memos(ctx: Arc<Ctx>, memos: Vec<(String, memos::Memo)>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;

    let mut pipe = redis::pipe();
    pipe.atomic();
    for (pda, memo) in &memos {
        let save: MemoSave = memo.into();
        let ser_memo = serde_json::to_string(&save).unwrap();
        pipe.set(format!("memo:{pda}"), &ser_memo).ignore();
        pipe.sadd("memos:index", pda).ignore();
    }

    pipe.query_async::<()>(&mut con).await?;

    let _ = ctx.tx.send(UiEvent::Stored(memos));

    Ok(())
}

pub async fn load_slot(ctx: Arc<Ctx>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;
    if let Some(slot) = con.get::<_, Option<Slot>>("slot").await? {
        tracing::info!("Loaded slot from cache. Value: {slot}");
        ctx.set_slot(slot, false).await;
    }

    Ok(())
}

pub async fn set_slot(ctx: Arc<Ctx>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;

    let slot = ctx.slot();
    let _: () = con.set("slot", slot).await?;

    Ok(())
}

pub async fn load_memos(ctx: Arc<Ctx>) -> Result<()> {
    let con = ctx.redis.lock().await;
    let mut con = con.get_multiplexed_async_connection().await?;
    let pdas: Vec<String> = con.smembers("memos:index").await?;

    if pdas.is_empty() {
        return Ok(());
    }

    let keys: Vec<String> = pdas.iter().map(|p| format!("memo:{p}")).collect();
    let values: Vec<Option<String>> = con.mget(&keys).await?;

    let values: Vec<Option<memos::Memo>> = values
        .into_iter()
        .map(|m| {
            let save: Option<MemoSave> = m.and_then(|s| {
                serde_json::from_str(&s)
                    .inspect_err(|e| tracing::error!("Error deserializing memo from redis: {e:?}"))
                    .ok()
            });
            save
        })
        .map(|m| m.map(Into::into))
        .collect();

    let memos = pdas
        .into_iter()
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
