use crate::{
    Ctx, UiEvent,
    persist::{self, store_memos},
};
use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use anchor_lang::AccountDeserialize;
use anyhow::Result;
use helius::types::{
    GetProgramAccountsV2Config, RpcTransactionsConfig, TransactionCommitment,
    TransactionSubscribeFilter, TransactionSubscribeOptions,
};
use solana_client::rpc_response::UiAccountData;
use solana_sdk::signature::Keypair;
use solana_system_interface::program as system_program;
use std::sync::{Arc, atomic::Ordering};
use tokio_stream::StreamExt;

pub async fn task_stream_chain(ctx: Arc<Ctx>) -> Result<()> {
    // Load the slot from cache (if it exists)
    persist::load_slot(ctx.clone()).await?;
    // Fetch any memos we have missed while offline.
    fetch_memos(&ctx, Some(ctx.slot.load(Ordering::Relaxed))).await?;
    // Load the memos from cache after so we can remove the items from the inbox as they're marked
    // as cached.
    crate::persist::load_memos(ctx.clone()).await?;

    let ws = ctx.helius.ws().unwrap();

    let config = RpcTransactionsConfig {
        filter: TransactionSubscribeFilter::standard(&memos::ID),
        options: TransactionSubscribeOptions {
            commitment: Some(TransactionCommitment::Finalized),
            ..Default::default()
        },
    };
    let (mut stream, _unsub) = ws.transaction_subscribe(config).await?;
    while let Some(n) = stream.next().await {
        fetch_memos(&ctx, Some(n.slot)).await?;
    }

    Ok(())
}

/// Fetches all memos, optionally requiring the response to reflect state at or after a given slot.
async fn fetch_memos(ctx: &Arc<Ctx>, changed_since_slot: Option<u64>) -> Result<()> {
    tracing::info!("Fetching memos. After slot: {changed_since_slot:?}");

    if let Some(slot) = changed_since_slot {
        ctx.slot.store(slot, Ordering::Relaxed);
    }

    let response = ctx
        .helius
        .rpc()
        .get_program_accounts_v2(
            memos::ID.to_string(),
            GetProgramAccountsV2Config {
                encoding: Some(helius::types::Encoding::Base64Zstd),
                changed_since_slot,
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

    tracing::info!("Fetched {} memos.", memos.len());
    let _ = ctx.tx.send(UiEvent::MemoInbox(memos.clone()));
    // Store the memos in redis
    tokio::spawn(store_memos(ctx.clone(), memos, changed_since_slot));

    Ok(())
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
