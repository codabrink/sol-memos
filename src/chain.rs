use anchor_lang::AccountDeserialize;
use anyhow::Result;
use helius::types::{
    GetProgramAccountsV2Config, RpcTransactionsConfig, TransactionSubscribeFilter,
    TransactionSubscribeOptions,
};
use serde_json::Value;
use solana_client::{rpc_config::UiAccountEncoding, rpc_response::UiAccountData};
use std::sync::Arc;
use tokio_stream::StreamExt;

use crate::{Ctx, UiEvent};

pub async fn stream_chain(app: Arc<Ctx>) -> Result<()> {
    let mut pagination_key = fetch_memos(&app, None).await?;

    let ws = app.helius.ws().unwrap();

    let config = RpcTransactionsConfig {
        filter: TransactionSubscribeFilter::standard(&memos::ID),
        options: TransactionSubscribeOptions::default(),
    };
    let (mut stream, _unsub) = ws.transaction_subscribe(config).await?;
    while let Some(_) = stream.next().await {
        pagination_key = fetch_memos(&app, pagination_key).await?;
    }

    Ok(())
}

async fn fetch_memos(ctx: &Ctx, pagination_key: Option<String>) -> Result<Option<String>> {
    let response = ctx
        .helius
        .rpc()
        .get_program_accounts_v2(
            memos::ID.to_string(),
            GetProgramAccountsV2Config {
                encoding: Some(helius::types::Encoding::Base64Zstd),
                pagination_key,
                ..Default::default()
            },
        )
        .await?;

    for gpa in response.accounts {
        let bytes = gpa
            .account
            .data
            .as_array()
            .and_then(|v| v.as_array())
            .and_then(|v: &[Value; 2]| v[0].as_str())
            .map(|v| UiAccountData::Binary(v.to_string(), UiAccountEncoding::Base64Zstd))
            .and_then(|v| v.decode())
            .expect("Data arrived in unexpected format.");

        let memo = memos::Memo::try_deserialize(&mut &*bytes)?;
        let _ = ctx.tx.send(UiEvent::NewMemo((gpa.pubkey, memo)));
    }

    Ok(response.pagination_key)
}
