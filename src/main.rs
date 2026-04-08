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
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{List, ListState};
use serde_json::Value;
use solana_client::rpc_config::UiAccountEncoding;
use solana_client::rpc_response::UiAccountData;
use solana_sdk::signature::{Keypair, read_keypair_file};
use solana_system_interface::program as system_program;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
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
    MemoPublished(usize),
    MemoInbox((String, memos::Memo)),
}

static ID: AtomicUsize = AtomicUsize::new(0);

struct App {
    ctx: Arc<Ctx>,
    exit: bool,
    pending_memo: String,
    rx: UnboundedReceiver<UiEvent>,

    outbox: Vec<(usize, Instant, memos::Memo)>,
    outbox_list_state: ListState,
}

struct Ctx {
    tx: UnboundedSender<UiEvent>,
    runtime: Runtime,
    helius: Helius,
    payer: Arc<Keypair>,
}

impl App {
    fn new() -> Result<Self> {
        let (tx, rx) = unbounded_channel();
        Ok(Self {
            ctx: Arc::new(Ctx::new(tx)?),
            exit: false,
            pending_memo: String::new(),
            rx,

            outbox: Vec::new(),
            outbox_list_state: ListState::default(),
        })
    }
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let left_width = 20;

        // Main body and text input on the bottom.
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(&[Constraint::Fill(1), Constraint::Length(3)])
            .split(frame.area());

        // Draw text input
        let input = Paragraph::new(self.pending_memo.as_str())
            .style(Style::default())
            .block(Block::bordered().title("Input"));
        frame.render_widget(input, outer_layout[1]);

        // Split the main body into two columns.
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(&[
                Constraint::Percentage(left_width),
                Constraint::Percentage(100 - left_width),
            ])
            .split(outer_layout[0]);

        // Split the left column into outbox and inbox
        let outbox_inbox_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(&[Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(column_layout[0]);

        self.draw_outbox(frame, outbox_inbox_layout[1]);
    }

    fn draw_outbox(&mut self, frame: &mut Frame, rect: Rect) {
        let items: Vec<_> = self
            .outbox
            .iter()
            .map(|(_addr, memo)| memo.memo.as_str())
            .collect();

        let list = List::new(items)
            .style(Color::White)
            .highlight_style(Modifier::REVERSED)
            .highlight_symbol("> ")
            .block(Block::bordered().title("Outbox"));

        frame.render_stateful_widget(list, rect, &mut self.outbox_list_state);
    }

    fn handle_events(&mut self) -> Result<()> {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                UiEvent::MemoInbox(memo) => {}
                UiEvent::MemoPublished(id) => {}
            }
        }

        match event::read()? {
            // it's important to check that the event is a key press event as
            // crossterm also emits key release and repeat events on Windows.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        };

        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => {
                self.exit = true;
            }
            KeyCode::Char(ch) => {
                self.pending_memo.push(ch);
            }
            KeyCode::Backspace => {
                self.pending_memo.pop();
            }
            KeyCode::Enter => {
                let memo = self.pending_memo.clone();
                self.pending_memo.clear();
                self.outbox.push((
                    ID.fetch_add(1, Ordering::SeqCst),
                    Instant::now(),
                    memos::Memo { memo },
                ));
            }
            _ => {}
        }
    }
}

impl Ctx {
    fn new(tx: UnboundedSender<UiEvent>) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .enable_io()
            .build()?;

        let helius = runtime.block_on(async {
            // Calling new_async is neccesary to be able to open a websocket on it.
            let helius = Helius::new_async(&var("HELIUS_KEY")?, HeliusCluster::Devnet)
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
            anyhow::Ok(helius)
        })?;

        let payer = read_keypair_file(&*shellexpand::tilde("~/.config/solana/id.json"))
            .expect("Example requires a keypair file");

        Ok(Self {
            tx,
            runtime,
            helius,
            payer: Arc::new(payer),
        })
    }
}

fn main() -> Result<()> {
    dotenv()?;

    let mut app = App::new()?;
    app.ctx.runtime.spawn(chain::stream_chain(app.ctx.clone()));

    ratatui::run(move |terminal| app.run(terminal))
}

async fn publish_memo(ctx: Arc<Ctx>, id: usize, memo: String) -> Result<()> {
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
        .await
        .unwrap();

    println!("published");
    ctx.tx.send(UiEvent::MemoPublished())

    Ok(())
}
