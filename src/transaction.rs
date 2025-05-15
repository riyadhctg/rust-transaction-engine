use dashmap::mapref::entry::Entry;
use log::warn;
use rust_decimal::Decimal;
use std::error::Error;

use crate::account::mutate_account_balance;
use crate::models::{
    Account, AccountsMap, Transaction, TransactionRecord, TransactionType, TransactionsMap,
};

pub fn handle_transaction(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client_id = transaction.client;

    // Check if account exists and is locked
    if let Some(account) = accounts.get(&client_id) {
        if account.locked
            && !matches!(
                transaction.tx_type,
                TransactionType::Dispute | TransactionType::Resolve | TransactionType::Chargeback
            )
        {
            warn!(
                "Transaction ignored: Account {} is locked (Tx ID: {})",
                client_id, transaction.tx
            );
            return Ok(());
        }
    }

    match transaction.tx_type {
        TransactionType::Deposit => handle_deposit(transaction, accounts, transactions),
        TransactionType::Withdrawal => handle_withdrawal(transaction, accounts, transactions),
        TransactionType::Dispute => handle_dispute(transaction, accounts, transactions),
        TransactionType::Resolve => handle_resolve(transaction, accounts, transactions),
        TransactionType::Chargeback => handle_chargeback(transaction, accounts, transactions),
    }
}

fn handle_deposit(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(amount) = transaction.amount {
        if amount <= Decimal::ZERO {
            return Ok(());
        }
    } else {
        return Ok(());
    }

    let client_id = transaction.client;

    if let Some(account) = accounts.get(&client_id) {
        if account.locked {
            warn!(
                "Deposit ignored: Account {} is locked (Tx ID: {})",
                client_id, transaction.tx
            );
            return Ok(());
        }
    }

    let mut account_entry = accounts.entry(client_id).or_insert_with(|| Account {
        client: client_id,
        ..Default::default()
    });

    if let Some(amount) = transaction.amount {
        if insert_transaction(transactions, transaction.tx, client_id, amount) {
            mutate_account_balance(&mut account_entry, amount, Decimal::ZERO, amount);
        } else {
            warn!(
                "Duplicate transaction ID {} for deposit - skipping (Client ID: {})",
                transaction.tx, client_id
            );
        }
    }

    Ok(())
}

fn handle_withdrawal(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(amount) = transaction.amount {
        if amount <= Decimal::ZERO {
            return Ok(());
        }
    } else {
        return Ok(());
    }

    let client_id = transaction.client;

    if let Some(account) = accounts.get(&client_id) {
        if account.locked {
            warn!(
                "Withdrawal ignored: Account {} is locked (Tx ID: {})",
                client_id, transaction.tx
            );
            return Ok(());
        }
    }

    let mut account_entry = accounts.entry(client_id).or_insert_with(|| Account {
        client: client_id,
        ..Default::default()
    });

    if let Some(amount) = transaction.amount {
        if account_entry.available >= amount {
            if insert_transaction(transactions, transaction.tx, client_id, -amount) {
                mutate_account_balance(&mut account_entry, -amount, Decimal::ZERO, -amount);
            } else {
                warn!(
                    "Duplicate transaction ID {} for withdrawal - skipping (Client ID: {})",
                    transaction.tx, client_id
                );
            }
        } else {
            warn!(
                "Insufficient funds for withdrawal. Client: {}, Tx: {}, Amount: {}, Available: {}",
                client_id, transaction.tx, amount, account_entry.available
            );
        }
    }

    Ok(())
}

fn handle_dispute(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client_id = transaction.client;
    let mut account_entry = accounts.entry(client_id).or_insert_with(|| Account {
        client: client_id,
        ..Default::default()
    });

    match transactions.get_mut(&transaction.tx) {
        Some(mut tx_record) if tx_record.client == client_id && !tx_record.disputed => {
            if tx_record.amount <= Decimal::ZERO {
                warn!(
                    "Dispute ignored: transaction {} is not a deposit (Client: {})",
                    transaction.tx, client_id
                );
                return Ok(());
            }

            let dispute_amount = tx_record.amount;
            tx_record.disputed = true;

            mutate_account_balance(
                &mut account_entry,
                -dispute_amount,
                dispute_amount,
                Decimal::ZERO,
            );
        }
        _ => {
            warn!(
                "Dispute failed. Transaction not found or already disputed. Tx: {}, Client: {}",
                transaction.tx, client_id
            );
        }
    }

    Ok(())
}

fn handle_resolve(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client_id = transaction.client;
    let mut account_entry = accounts.entry(client_id).or_insert_with(|| Account {
        client: client_id,
        ..Default::default()
    });

    match transactions.get_mut(&transaction.tx) {
        Some(mut tx_record) if tx_record.client == client_id && tx_record.disputed => {
            if tx_record.amount <= Decimal::ZERO {
                warn!(
                    "Resolve ignored: transaction {} is not a deposit (Client: {})",
                    transaction.tx, client_id
                );
                return Ok(());
            }

            let resolve_amount = tx_record.amount;
            tx_record.disputed = false;

            mutate_account_balance(
                &mut account_entry,
                resolve_amount,
                -resolve_amount,
                Decimal::ZERO,
            );
        }
        Some(_) => {
            warn!(
                "Resolve ignored. Transaction not under dispute. Tx: {}, Client: {}",
                transaction.tx, client_id
            );
        }
        None => {
            warn!(
                "Resolve failed. Transaction not found. Tx: {}, Client: {}",
                transaction.tx, client_id
            );
        }
    }

    Ok(())
}

fn handle_chargeback(
    transaction: Transaction,
    accounts: &AccountsMap,
    transactions: &TransactionsMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client_id = transaction.client;
    let mut account_entry = accounts.entry(client_id).or_insert_with(|| Account {
        client: client_id,
        ..Default::default()
    });

    match transactions.get_mut(&transaction.tx) {
        Some(mut tx_record) if tx_record.client == client_id && tx_record.disputed => {
            if tx_record.amount <= Decimal::ZERO {
                warn!(
                    "Chargeback ignored: transaction {} is not a deposit (Client: {})",
                    transaction.tx, client_id
                );
                return Ok(());
            }

            let chargeback_amount = tx_record.amount;
            tx_record.disputed = false;
            account_entry.locked = true;

            mutate_account_balance(
                &mut account_entry,
                Decimal::ZERO,
                -chargeback_amount,
                -chargeback_amount,
            );
        }
        Some(_) => {
            warn!(
                "Chargeback ignored. Transaction not under dispute. Tx: {}, Client: {}",
                transaction.tx, client_id
            );
        }
        None => {
            warn!(
                "Chargeback failed. Transaction not found. Tx: {}, Client: {}",
                transaction.tx, client_id
            );
        }
    }

    Ok(())
}

/// Insert transaction into global map if not duplicate
pub fn insert_transaction(tx_map: &TransactionsMap, tx: u32, client: u16, amount: Decimal) -> bool {
    match tx_map.entry(tx) {
        Entry::Occupied(_) => false,
        Entry::Vacant(entry) => {
            entry.insert(TransactionRecord {
                client,
                amount,
                disputed: false,
            });
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::sync::Arc;

    fn setup_test_environment() -> (Arc<AccountsMap>, Arc<TransactionsMap>) {
        let accounts = Arc::new(AccountsMap::new());
        let transactions = Arc::new(TransactionsMap::new());

        (accounts, transactions)
    }

    fn new_transaction(
        tx_type: TransactionType,
        client: u16,
        tx: u32,
        amount: Option<Decimal>,
    ) -> Transaction {
        Transaction {
            tx_type,
            client,
            tx,
            amount,
        }
    }

    #[tokio::test]
    async fn test_deposit_valid() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::from(100));
        assert_eq!(account.total, Decimal::from(100));
        assert_eq!(account.held, Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_withdrawal_sufficient_funds() {
        let (accounts, transactions) = setup_test_environment();
        // Initial deposit
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        // Withdrawal
        let withdrawal =
            new_transaction(TransactionType::Withdrawal, 1, 101, Some(Decimal::from(50)));
        handle_transaction(withdrawal, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::from(50));
        assert_eq!(account.total, Decimal::from(50));
    }

    #[tokio::test]
    async fn test_withdrawal_insufficient_funds() {
        let (accounts, transactions) = setup_test_environment();
        let withdrawal =
            new_transaction(TransactionType::Withdrawal, 1, 100, Some(Decimal::from(50)));
        handle_transaction(withdrawal, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::ZERO);
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.total, Decimal::ZERO);
    }

    #[tokio::test]
    async fn test_dispute_on_deposit() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        let dispute = new_transaction(TransactionType::Dispute, 1, 100, None);
        handle_transaction(dispute, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::ZERO);
        assert_eq!(account.held, Decimal::from(100));

        let tx_record = transactions.get(&100).unwrap();
        assert!(tx_record.disputed);
    }

    #[tokio::test]
    async fn test_resolve_dispute() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        let dispute = new_transaction(TransactionType::Dispute, 1, 100, None);
        handle_transaction(dispute, &accounts, &transactions).unwrap();

        let resolve = new_transaction(TransactionType::Resolve, 1, 100, None);
        handle_transaction(resolve, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::from(100));
        assert_eq!(account.held, Decimal::ZERO);

        let tx_record = transactions.get(&100).unwrap();
        assert!(!tx_record.disputed);
    }

    #[tokio::test]
    async fn test_chargeback_dispute() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        let dispute = new_transaction(TransactionType::Dispute, 1, 100, None);
        handle_transaction(dispute, &accounts, &transactions).unwrap();

        let chargeback = new_transaction(TransactionType::Chargeback, 1, 100, None);
        handle_transaction(chargeback, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.held, Decimal::ZERO);
        assert_eq!(account.total, Decimal::ZERO);
        assert!(account.locked);
    }

    #[tokio::test]
    async fn test_duplicate_transaction_id() {
        let (accounts, transactions) = setup_test_environment();
        let deposit1 = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit1, &accounts, &transactions).unwrap();

        let deposit2 = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(200)));
        handle_transaction(deposit2, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.available, Decimal::from(100));
    }

    #[tokio::test]
    async fn test_locked_account_ignores_transactions() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        let dispute = new_transaction(TransactionType::Dispute, 1, 100, None);
        handle_transaction(dispute, &accounts, &transactions).unwrap();

        let chargeback = new_transaction(TransactionType::Chargeback, 1, 100, None);
        handle_transaction(chargeback, &accounts, &transactions).unwrap();

        // Try another deposit on locked account
        let new_deposit =
            new_transaction(TransactionType::Deposit, 1, 101, Some(Decimal::from(50)));
        handle_transaction(new_deposit, &accounts, &transactions).unwrap();

        let account = accounts.get(&1).unwrap();
        assert_eq!(account.total, Decimal::ZERO); // Should not have changed
    }

    #[tokio::test]
    async fn test_negative_amount_deposit_ignored() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, Some(Decimal::from(-100)));
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        assert!(accounts.get(&1).is_none());
    }

    #[tokio::test]
    async fn test_missing_amount_ignored() {
        let (accounts, transactions) = setup_test_environment();
        let deposit = new_transaction(TransactionType::Deposit, 1, 100, None);
        handle_transaction(deposit, &accounts, &transactions).unwrap();

        assert!(accounts.get(&1).is_none());
    }
}
