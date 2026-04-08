mod chain;

use anchor_client::{Client, Cluster, CommitmentConfig, Signer};
use anchor_lang::prelude::*;
use anyhow::Result;
use dotenvy::{dotenv, var};
use helius::types::{Cluster as HeliusCluster, GetProgramAccountsV2Config};
use helius::types::{
    RpcTransactionsConfig, TransactionSubscribeFilter, TransactionSubscribeOptions,
};
use helius::{Helius, error::HeliusError};
use serde_json::Value;
use solana_client::rpc_config::UiAccountEncoding;
use solana_client::rpc_response::UiAccountData;
use solana_sdk::signature::{Keypair, read_keypair_file};
use solana_system_interface::program as system_program;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_stream::StreamExt;

use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};

// #[derive(Deserialize)]
// struct MemosIdl {
// pub address: Pubkey,
// }

enum UiEvent {
    NewMemo((String, memos::Memo)),
}

struct App {
    ui_rx: UnboundedReceiver<UiEvent>,
    ui_tx: UnboundedSender<UiEvent>,
    runtime: Runtime,
    helius: Helius,
    payer: Arc<Keypair>,
}

impl App {
    fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread().build()?;
        let (ui_tx, ui_rx) = unbounded_channel();

        let helius_key = var("HELIUS_KEY");
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
        let payer = read_keypair_file(&*shellexpand::tilde("~/.config/solana/id.json"))
            .expect("Example requires a keypair file");
        Ok(Self {
            ui_rx,
            ui_tx,
            runtime,
            helius,
            payer: Arc::new(payer),
        })
    }

    fn run(&self, terminal: &mut DefaultTerminal) -> Result<()> {
        Ok(())
    }
}

fn main() -> Result<()> {
    dotenv()?;

    let app = Arc::new(App::new()?);
    app.runtime.spawn(chain::stream_chain(app.clone()));

    ratatui::run(move |terminal| app.run(terminal))
}

async fn publish_memo(app: Arc<App>, memo: String) -> Result<()> {
    let memo_kp = Arc::new(Keypair::new());
    let memo_pubkey = memo_kp.pubkey();

    let client = Client::new_with_options(
        Cluster::Devnet,
        app.payer.clone(),
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
        .await
        .unwrap();

    println!("published");

    Ok(())
}
