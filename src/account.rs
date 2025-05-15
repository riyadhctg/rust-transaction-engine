use rust_decimal::{Decimal, RoundingStrategy};
use std::error::Error;
use std::io;

use crate::models::{Account, AccountsMap};

/// Truncate decimal to 4 digits using zero rounding strategy
pub fn truncate_to_4(amount: Decimal) -> Decimal {
    amount.round_dp_with_strategy(4, RoundingStrategy::ToZero)
}

/// Mutate account balance fields and truncate to 4 digits
pub fn mutate_account_balance(
    account: &mut Account,
    available_delta: Decimal,
    held_delta: Decimal,
    total_delta: Decimal,
) {
    account.available = truncate_to_4(account.available + available_delta);
    account.held = truncate_to_4(account.held + held_delta);
    account.total = truncate_to_4(account.total + total_delta);
}

/// Output final account balances sorted by client ID
pub fn output_accounts(accounts: &AccountsMap) -> Result<(), Box<dyn Error + Send + Sync>> {
    let entries: Vec<_> = accounts.iter().map(|e| e.value().clone()).collect();
    let mut wtr = csv::Writer::from_writer(io::stdout());
    for entry in entries {
        wtr.serialize(entry)?;
    }
    wtr.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn test_truncate_to_4() {
        assert_eq!(
            truncate_to_4(Decimal::from_str("123.45678").unwrap()),
            Decimal::from_str("123.4567").unwrap()
        );

        assert_eq!(
            truncate_to_4(Decimal::from_str("123.45").unwrap()),
            Decimal::from_str("123.45").unwrap()
        );
    }

    #[test]
    fn test_mutate_account_balance() {
        let mut account = Account {
            client: 1,
            available: Decimal::from(100),
            held: Decimal::from(50),
            total: Decimal::from(150),
            locked: false,
        };

        mutate_account_balance(
            &mut account,
            Decimal::from(10),
            Decimal::from(5),
            Decimal::from(15),
        );

        assert_eq!(account.available, Decimal::from(110));
        assert_eq!(account.held, Decimal::from(55));
        assert_eq!(account.total, Decimal::from(165));
    }
}
