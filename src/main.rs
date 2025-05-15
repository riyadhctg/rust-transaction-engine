use csv_async::{AsyncReaderBuilder, Trim};
use env_logger::Env;
use futures::{StreamExt, TryStreamExt};
use log::{self, error};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::fs::File;
use tokio::io::BufReader;
use tokio::sync::mpsc;

use crate::account::output_accounts;
use crate::models::{AccountsMap, Transaction, TransactionsMap};
use crate::transaction::handle_transaction;

mod account;
mod models;
mod transaction;

#[tokio::main]
async fn main() {
    let env = Env::default().filter_or("RUST_LOG", "info");
    env_logger::Builder::from_env(env).init();
    if let Err(e) = run().await {
        error!("Application error: {}", e);
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("Usage: cargo run -- transactions.csv > accounts.csv".into());
    }
    let input_path = &args[1];
    let file = File::open(input_path).await?;
    let reader = BufReader::new(file);

    // Shared thread-safe maps for accounts and transactions
    let accounts: Arc<AccountsMap> = Arc::new(models::AccountsMap::new());
    let transactions: Arc<TransactionsMap> = Arc::new(models::TransactionsMap::new());

    const CONCURRENCY_LIMIT: usize = 50;

    // Each client has a dedicated channel to process transactions sequentially
    let senders: Arc<Mutex<HashMap<u16, mpsc::Sender<Transaction>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let accounts_clone = Arc::clone(&accounts);
    let transactions_clone = Arc::clone(&transactions);
    let senders_clone = Arc::clone(&senders);

    // Stream CSV transactions line-by-line
    let csv_reader = AsyncReaderBuilder::new()
        .trim(Trim::All)
        .flexible(true)
        .create_deserializer(reader)
        .into_deserialize::<Transaction>();

    csv_reader
        .map(|tx| tx.map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>))
        .try_for_each(move |transaction| {
            let senders = Arc::clone(&senders_clone);
            let accounts = Arc::clone(&accounts_clone);
            let transactions = Arc::clone(&transactions_clone);
            async move {
                match transaction.tx_type {
                    models::TransactionType::Deposit | models::TransactionType::Withdrawal => {
                        if transaction
                            .amount
                            .is_none_or(|a| a <= rust_decimal::Decimal::ZERO)
                        {
                            log::warn!(
                                "Invalid or missing amount in deposit/withdrawal: {:?}",
                                transaction
                            );
                            return Ok(());
                        }
                    }
                    _ => {}
                }

                let client_id = transaction.client;

                let sender = {
                    let mut senders_lock = senders.lock().unwrap();

                    // Create a new channel per client if not already present
                    senders_lock
                        .entry(client_id)
                        .or_insert_with(|| {
                            let (tx_chan, rx_chan) = mpsc::channel(CONCURRENCY_LIMIT);
                            let accounts_clone = Arc::clone(&accounts);
                            let transactions_clone = Arc::clone(&transactions);
                            tokio::spawn(async move {
                                process_client_transactions(
                                    rx_chan,
                                    accounts_clone,
                                    transactions_clone,
                                )
                                .await;
                            });
                            tx_chan
                        })
                        .clone()
                };

                // Send transaction to client's channel
                if sender.send(transaction).await.is_err() {
                    log::warn!(
                        "Failed to send transaction to client {}'s channel",
                        client_id
                    );
                }

                Ok(())
            }
        })
        .await?;

    output_accounts(&accounts)?;
    Ok(())
}

/// Process all transactions for one client sequentially.
///
/// Ensures that all operations for a given client are handled in order.
async fn process_client_transactions(
    mut rx: mpsc::Receiver<Transaction>,
    accounts: Arc<AccountsMap>,
    transactions: Arc<TransactionsMap>,
) {
    while let Some(tx) = rx.recv().await {
        if let Err(e) = handle_transaction(tx, &accounts, &transactions) {
            log::warn!("Error handling transaction: {:?}", e);
        }
    }
}
