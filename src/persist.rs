use crate::Ctx;
use redis::AsyncCommands;
use std::sync::Arc;

pub async fn store_memos(
    ctx: Arc<Ctx>,
    memos: Vec<(String, memos::Memo)>,
    pagination: Option<String>,
) {
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
        pipe.set(format!("memo-{pda}"), &memo.memo).ignore();
    }
    if let Some(ref key) = pagination {
        pipe.set("pagination", key).ignore();
    }

    if let Err(e) = pipe.query_async::<()>(&mut con).await {
        tracing::error!("Redis error: {e}");
    }
}

pub async fn query_memos(ctx: Arc<Ctx>) -> (Vec<(String, String)>, Option<String>) {
    let con = ctx.redis.lock().await;
    let mut con = match con.get_multiplexed_async_connection().await {
        Ok(con) => con,
        Err(e) => {
            tracing::error!("Redis connection error: {e}");
            return (vec![], None);
        }
    };

    let keys: Vec<String> = match con.keys("memo-*").await {
        Ok(keys) => keys,
        Err(e) => {
            tracing::error!("Redis error: {e}");
            return (vec![], None);
        }
    };

    let pagination: Option<String> = match con.get("pagination").await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Redis error: {e}");
            return (vec![], None);
        }
    };

    if keys.is_empty() {
        return (vec![], pagination);
    }

    let values: Vec<String> = match con.mget(&keys).await {
        Ok(values) => values,
        Err(e) => {
            tracing::error!("Redis error: {e}");
            return (vec![], pagination);
        }
    };

    let memos = keys
        .into_iter()
        .map(|k| k.trim_start_matches("memo-").to_string())
        .zip(values)
        .collect();

    (memos, pagination)
}
