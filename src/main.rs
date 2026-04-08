use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use anchor_lang::prelude::*;
use anyhow::Result;
use base64::Engine;
use base64::alphabet::{STANDARD, URL_SAFE};
use base64::prelude::{BASE64_STANDARD, BASE64_STANDARD_NO_PAD, BASE64_URL_SAFE};
use dotenvy::{dotenv, var};
use helius::types::{
    Cluster as HeliusCluster, CreateWebhookRequest, GetProgramAccountsV2Config, TransactionType,
    WebhookType,
};
use helius::types::{
    RpcTransactionsConfig, TransactionSubscribeFilter, TransactionSubscribeOptions,
};
use helius::{Helius, error::HeliusError};
use serde_json::Value;
use solana_client::rpc_config::UiAccountEncoding;
use solana_client::rpc_response::UiAccountData;
use solana_sdk::signature::{Keypair, read_keypair_file};
use solana_system_interface::program as system_program;
use solana_transaction_status::{
    EncodedTransaction, UiInstruction, UiMessage, UiParsedInstruction,
};
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;

// #[derive(Deserialize)]
// struct MemosIdl {
// pub address: Pubkey,
// }

#[tokio::main]
async fn main() -> Result<()> {
    dotenv()?;

    // let memo_idl_str = std::fs::read_to_string("./memos/target/idl/memos.json")
    // .context("Unable to load memos idl")?;
    // let memos_idl: MemosIdl = serde_json::from_str(&memo_idl_str)?;

    let payer = read_keypair_file(&*shellexpand::tilde("~/.config/solana/id.json"))
        .expect("Example requires a keypair file");
    let cluster = Cluster::Devnet;

    let payer = Arc::new(payer);
    let client = Client::new_with_options(
        cluster.clone(),
        payer.clone(),
        CommitmentConfig::processed(),
    );

    let program = client.program(memos::ID)?;

    tokio::task::spawn(async move {
        loop {
            let memo = Arc::new(Keypair::new());
            let memo_pubkey = memo.pubkey();

            program
                .request()
                .signer(memo)
                .accounts(memos::accounts::StoreMemo {
                    memo: memo_pubkey,
                    signer: program.payer(),
                    system_program: system_program::ID,
                })
                .args(memos::instruction::StoreMemo {
                    text: "Hello there".to_string(),
                })
                .send()
                .await
                .unwrap();

            println!("published");

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    let helius_key = var("HELIUS_KEY");
    println!("Helius key: {helius_key:?}");

    let helius = Helius::new_async(&helius_key?, HeliusCluster::Devnet)
        .await
        .inspect_err(|e| {
            if let HeliusError::Tungstenite(e) = e {
                if let tokio_tungstenite::tungstenite::Error::Http(e) = e {
                    let body = e.body().as_ref().unwrap();
                    println!("{}", String::from_utf8(body.clone()).unwrap());
                };
            }
        })
        .unwrap();

    let response = helius
        .rpc()
        .get_program_accounts_v2(
            memos::ID.to_string(),
            GetProgramAccountsV2Config {
                encoding: Some(helius::types::Encoding::Base64Zstd),
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

        let memo = memos::Memo::try_deserialize(&mut &*bytes).unwrap();
        println!("{}: {}", gpa.pubkey, memo.memo);
    }

    let ws = helius.ws().unwrap();

    let config = RpcTransactionsConfig {
        filter: TransactionSubscribeFilter::standard(&memos::ID),
        options: TransactionSubscribeOptions::default(),
    };
    let (mut stream, _unsub) = ws.transaction_subscribe(config).await?;
    while let Some(event) = stream.next().await {
        println!("streamed {event:?}");
    }

    Ok(())
}
