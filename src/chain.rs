use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use anchor_lang::AccountDeserialize;
use anyhow::Result;
use helius::types::{
    GetProgramAccountsV2Config, RpcTransactionsConfig, TransactionSubscribeFilter,
    TransactionSubscribeOptions,
};
use solana_client::rpc_response::UiAccountData;
use solana_sdk::signature::Keypair;
use solana_system_interface::program as system_program;
use std::sync::Arc;
use tokio_stream::StreamExt;

use crate::{Ctx, UiEvent, persist::store_memos};

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

/// Fetches all of the memos after a provided pagination key
async fn fetch_memos(ctx: &Arc<Ctx>, pagination_key: Option<String>) -> Result<Option<String>> {
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

    let mut memos = vec![];
    for gpa in response.accounts {
        let account_data: UiAccountData = serde_json::from_value(gpa.account.data)?;
        let bytes = account_data.decode().expect("Unexpected format");

        let memo = memos::Memo::try_deserialize(&mut &*bytes)?;
        memos.push((gpa.pubkey, memo));
    }

    let _ = ctx.tx.send(UiEvent::MemoInbox(memos.clone()));
    // Store the memos in redis
    tokio::spawn(store_memos(
        ctx.clone(),
        memos,
        response.pagination_key.clone(),
    ));

    Ok(response.pagination_key)
}

/// Publishes a memo to the chain on the memos program.
pub async fn publish_memo(ctx: Arc<Ctx>, id: usize, memo: String) -> Result<()> {
    let memo_kp = Arc::new(Keypair::new());
    let memo_pubkey = memo_kp.pubkey();

    let client = Client::new_with_options(
        Cluster::Devnet,
        ctx.payer.clone(),
        CommitmentConfig::processed(),
    );
    let program = client.program(memos::ID)?;

    program
        .request()
        .signer(memo_kp)
        .accounts(memos::accounts::StoreMemo {
            memo: memo_pubkey,
            signer: program.payer(),
            system_program: system_program::ID,
        })
        .args(memos::instruction::StoreMemo { text: memo })
        .send()
        .await?;

    ctx.tx.send(UiEvent::MemoPublished(id))?;

    Ok(())
}
