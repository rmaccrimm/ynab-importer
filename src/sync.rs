use crate::db::{budget, transaction};

use super::db::account;
use chrono::NaiveDate;
use rusqlite::Connection;

use anyhow::Result;
use ynab_api::apis::{configuration::Configuration, transactions_api::get_transactions_by_account};

pub async fn sync_transactions(conn: &Connection, api_config: &Configuration) -> Result<()> {
    let accounts = account::get_all(conn)?;
    for acc in accounts {
        let budg = budget::get(conn, acc.budget_id)?;
        let response = get_transactions_by_account(
            api_config,
            &budg.uuid.hyphenated().to_string(),
            &acc.uuid.hyphenated().to_string(),
            None,
            None,
            None,
        )
        .await?;
        println!(
            "Storing {} transactions for account {}",
            response.data.transactions.len(),
            acc.name
        );
        for t in response.data.transactions {
            transaction::create_if_not_exists(
                conn,
                acc.id,
                t.amount,
                NaiveDate::parse_from_str(&t.date, "%Y-%m-%d")?,
            )?;
        }
    }
    Ok(())
}
