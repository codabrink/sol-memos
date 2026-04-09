use crate::persist::set_slot;
use crate::*;
use anchor_lang::prelude::*;
use anyhow::Result;
use crossbeam_channel::Sender;
use dotenvy::var;
use helius::types::Cluster as HeliusCluster;
use helius::{Helius, error::HeliusError};
use solana_sdk::signature::{Keypair, read_keypair_file};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;

pub struct Ctx {
    pub tx: Sender<UiEvent>,
    pub runtime: Runtime,
    pub helius: Helius,
    pub payer: Arc<Keypair>,
    pub redis: Arc<TokioMutex<redis::Client>>,
    slot: AtomicU64,
}

impl Ctx {
    pub fn new(tx: Sender<UiEvent>) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .enable_io()
            .build()?;

        let Ok(redis_url) = var("REDIS_URL") else {
            eprintln!("Missing REDIS_URL env var.");
            std::process::exit(1);
        };
        let Ok(helius_key) = var("HELIUS_KEY") else {
            eprintln!("Missing HELIUS_KEY env var.");
            std::process::exit(1);
        };

        let redis = redis::Client::open(redis_url)?;

        let helius = runtime.block_on(async {
            // Calling new_async is neccesary to be able to open a websocket on it.
            let helius = Helius::new_async(&helius_key, HeliusCluster::Devnet)
                .await
                // Some errors need to be decoded...
                .inspect_err(|e| {
                    if let HeliusError::Tungstenite(e) = e {
                        if let tokio_tungstenite::tungstenite::Error::Http(e) = e {
                            let body = e.body().as_ref().unwrap();
                            println!("{}", String::from_utf8(body.clone()).unwrap());
                        };
                    }
                })?;
            anyhow::Ok(helius)
        })?;

        let Ok(payer) = read_keypair_file(&*shellexpand::tilde("~/.config/solana/id.json")) else {
            eprintln!("Missing payer keypair at ~/.config/solana/id.json");
            std::process::exit(1);
        };

        Ok(Self {
            tx,
            runtime,
            helius,
            payer: Arc::new(payer),
            redis: Arc::new(TokioMutex::new(redis)),
            slot: AtomicU64::new(0),
        })
    }

    pub fn slot(&self) -> u64 {
        self.slot.load(Ordering::Relaxed)
    }

    pub async fn set_slot(self: &Arc<Self>, slot: u64, store: bool) {
        self.slot.store(slot, Ordering::Relaxed);

        if !store {
            return;
        }

        if let Err(err) = set_slot(self.clone()).await {
            tracing::error!("Unable to store slot in redis: {err:?}");
        };
    }
}
