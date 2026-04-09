use crate::{
    Ctx, UiEvent,
    persist::{self, store_memos},
    wrapped::WrappedStream,
};
use anchor_client::{Client, Cluster, CommitmentConfig};
use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use helius::types::{
    GetProgramAccountsV2Config, RpcTransactionsConfig, TransactionCommitment,
    TransactionSubscribeFilter, TransactionSubscribeOptions,
};
use solana_client::rpc_response::UiAccountData;
use solana_system_interface::program as system_program;
use std::{sync::Arc, time::Duration};
use tokio_stream::StreamExt;

pub async fn task_stream_chain(ctx: Arc<Ctx>) -> Result<()> {
    loop {
        if let Err(err) = task_stream_chain_inner(&ctx).await {
            tracing::error!("Streaming error: {err:?}");
        };
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn task_stream_chain_inner(ctx: &Arc<Ctx>) -> Result<()> {
    // Load the slot from cache (if it exists)
    persist::load_slot(ctx.clone()).await?;
    crate::persist::load_memos(ctx.clone()).await?;

    let ws = ctx.helius.ws().context("Missing websocket feature.")?;

    let config = RpcTransactionsConfig {
        filter: TransactionSubscribeFilter::standard(&memos::ID),
        options: TransactionSubscribeOptions {
            commitment: Some(TransactionCommitment::Finalized),
            ..Default::default()
        },
    };
    let (stream, _unsub) = ws.transaction_subscribe(config).await?;
    let mut stream = WrappedStream::new(stream);

    // There's a tiny race condition where the slot can go backwards, but that's not necessarily a bad thing.
    // Better to potentially download a memo twice, than to potentially miss a memo.
    stream.inject(ctx.slot());

    while let Some(notify) = stream.next().await {
        fetch_memos(&ctx).await?;
        ctx.set_slot(notify.slot(), true).await;
    }

    Ok(())
}

/// Fetches all memos, optionally requiring the response to reflect state at or after a given slot.
async fn fetch_memos(ctx: &Arc<Ctx>) -> Result<()> {
    let slot = ctx.slot();
    tracing::info!("Fetching memos. After slot: {slot:?}");

    let response = ctx
        .helius
        .rpc()
        .get_program_accounts_v2(
            memos::ID.to_string(),
            GetProgramAccountsV2Config {
                encoding: Some(helius::types::Encoding::Base64Zstd),
                changed_since_slot: Some(slot),
                ..Default::default()
            },
        )
        .await?;

    let mut memos = vec![];
    for gpa in response.accounts {
        let account_data: UiAccountData = serde_json::from_value(gpa.account.data)?;
        let bytes = account_data.decode().expect("Unexpected format");

        let Ok(memo) = memos::Memo::try_deserialize(&mut &*bytes)
            .inspect_err(|e| tracing::error!("Unable to deserialiize memo from the chain: {e:?}"))
        else {
            continue;
        };
        memos.push((gpa.pubkey, memo));
    }

    tracing::info!("Fetched {} memos.", memos.len());
    let _ = ctx.tx.send(UiEvent::MemoInbox(memos.clone()));
    // Store the memos in redis
    tokio::spawn(store_memos(ctx.clone(), memos));

    Ok(())
}

/// Publishes a memo to the chain on the memos program.
pub async fn publish_memo(ctx: Arc<Ctx>, id: usize, memo: String) -> Result<()> {
    let mut i = 0;

    while let Err(err) = publish_memo_inner(&ctx, id, &memo).await {
        tracing::error!("Error publishing memo {memo}. {err:?} Retrying...");
        i += 1;
        tokio::time::sleep(Duration::from_secs(i + 1)).await;
    }

    Ok(())
}

pub async fn publish_memo_inner(ctx: &Ctx, id: usize, memo: &str) -> Result<()> {
    let client = Client::new_with_options(
        Cluster::Devnet,
        ctx.payer.clone(),
        CommitmentConfig::processed(),
    );
    let program = client.program(memos::ID)?;
    let payer = program.payer();

    let (counter_pda, _) =
        solana_sdk::pubkey::Pubkey::find_program_address(&[payer.as_ref(), b"counter"], &memos::ID);

    let count = program
        .account::<memos::MemoCounter>(counter_pda)
        .await
        .map(|c| c.count)
        .unwrap_or(0);

    let (memo_pda, _) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[payer.as_ref(), b"memo", &count.to_le_bytes()],
        &memos::ID,
    );

    program
        .request()
        .accounts(memos::accounts::StoreMemo {
            counter: counter_pda,
            memo: memo_pda,
            signer: payer,
            system_program: system_program::ID,
        })
        .args(memos::instruction::StoreMemo {
            text: memo.to_string(),
        })
        .send()
        .await?;

    ctx.tx.send(UiEvent::MemoPublished(id))?;

    Ok(())
}
