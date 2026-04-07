use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use anchor_lang::prelude::*;
use anyhow::Result;
use dotenvy::{dotenv, var};
use helius::types::Cluster as HeliusCluster;
use helius::{Helius, error::HeliusError};
use solana_sdk::signature::{Keypair, read_keypair_file};
use solana_system_interface::program as system_program;
use std::sync::Arc;
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

    let helius_key = var("HELIUS_KEY");
    println!("Helius key: {helius_key:?}");

    let helius = Helius::new(&helius_key?, HeliusCluster::Devnet)
        .inspect_err(|e| {
            if let HeliusError::Tungstenite(e) = e {
                if let tokio_tungstenite::tungstenite::Error::Http(e) = e {
                    let body = e.body().as_ref().unwrap();
                    println!("{}", String::from_utf8(body.clone()).unwrap());
                };
            }
        })
        .unwrap();

    let accounts = helius.connection().get_program_accounts(&memos::ID)?;
    for (address, account) in accounts {
        let memo = memos::Memo::try_deserialize(&mut &*account.data)?;
        println!("{}", memo.memo);
    }

    let account = helius.connection().get_account(&memo_pubkey)?;
    let memo = memos::Memo::try_deserialize(&mut &*account.data)?;
    println!("{}", memo.memo);

    // let ws = helius.ws().unwrap();
    // let (mut stream, _unsub) = ws.account_subscribe(&memos::ID, None).await?;
    // while let Some(event) = stream.next().await {
    // println!("{event:?}");
    // }

    Ok(())
}
