# Rust Transaction Engine

A toy transaction processing engine written in Rust for handling deposits, withdrawals, disputes, resolves, and chargebacks.

---

## 📌 Overview

This application processes a stream of financial transactions from a CSV file and updates account balances accordingly. It supports:

- **Deposits** and **Withdrawals**
- **Disputes**, **Resolutions**, and **Chargebacks**
- Thread-safe concurrency with per-client transaction queues
- Logging, error handling, and unit tests

It outputs final client account balances in CSV format.

---

## 🛠️ How It Works

1. **Input**: Reads a CSV file containing transactions.
2. **Processing**:
   - Parses each line into a `Transaction` struct.
   - Routes each transaction via a per-client channel to be processed sequentially.
   - Updates account balances or modifies transaction states accordingly.
3. **Output**: Prints the final state of all accounts in CSV format (unsorted)

---

## 💭 Key Assumptions and Notes on Business Logic

1. **Dispute** occurs only for **Depostis**
2. **Transaction** data comes in chronologically
3. **Amount** is not rounded but truncated at specific decimal precision (i.e., 4)
4. For invalid transactions (e.g., invalid input type), it does not error out but logs the issue
5. The below table summarizes how different transactions are treated

| **Transaction Type** | **available Δ** | **held Δ**    | **total Δ**   | **Locks Account?** |
|----------------------|------------------|---------------|---------------|--------------------|
| Deposit              | +amount          | 0             | +amount       | ❌                 |
| Withdrawal           | -amount          | 0             | -amount       | ❌                 |
| Dispute              | -amount          | +amount       | 0             | ❌                 |
| Resolve              | +amount          | -amount       | 0             | ❌                 |
| Chargeback           | 0                | -amount       | -amount       | ✅                 |


---

## 📁 Project Structure

```bash
src/
├── main.rs          # Entry point; reads input, sets up concurrency, runs 
├── account.rs       # Account balance mutation and output logic
├── transaction.rs   # Transaction handling logic
├── models.rs        # Data structures and types (Account, Transaction, etc.)
```

---

## 📦 Dependencies

- `rust_decimal`: For precise decimal math
- `dashmap`: For thread-safe hash maps
- `csv` / `csv_async`: For reading transaction data
- `tokio`: Async runtime
- `log` / `env_logger`: For logging
- `serde`: For CSV deserialization

---

## 🚀 Usage

### Build & Run

```bash
cargo build --release
```

### Execute

```bash
RUST_LOG=info cargo run -- transactions.csv > accounts.csv
```

Where:

- `transactions.csv` is your input file containing transaction records.
- `accounts.csv` will contain the final computed account balances.

---

## 📄 Input Format

Each line represents one of the following transaction types:

```csv
type,client,tx,amount
deposit,1,1,1.0
withdrawal,1,2,0.5
dispute,1,1
resolve,1,1
chargeback,1,1
```

> `amount` is optional except for `deposit` and `withdrawal`.

---

## ✅ Output Format

Final account balances are printed to stdout (or redirected to a file):

```csv
client,available,held,total,locked
1,1.0,0.5,1.5,false
```

---

## 🧪 Testing

Run the full test suite:

```bash
cargo test
```

All major transaction logic is covered, including:

- Valid deposit/withdrawal
- Dispute lifecycle (dispute → resolve/chargeback)
- Duplicate transaction IDs
- Locked account behavior
- Invalid or missing amounts

---

