use rusqlite;
use rusqlite::{params, Connection, OptionalExtension};
use ynab_api::models::Account;
use ynab_api::models::BudgetSummary;

pub mod config {
    use super::*;

    pub const USER_ID: &str = "user_id";
    pub const ACCESS_TOKEN: &str = "access_token";
    pub const TRANSACTION_DIR: &str = "transaction_dir";

    // Set the key value pair in configuration table
    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<usize, rusqlite::Error> {
        conn.execute(
            "INSERT INTO configuration(key, value) VALUES (?1, ?2) \
            ON CONFLICT(key) DO UPDATE SET value=?2;",
            params![key, value],
        )
    }

    // Get a value from configuration table
    pub fn get(conn: &Connection, key: &str) -> Result<String, rusqlite::Error> {
        conn.prepare("SELECT value FROM configuration WHERE key=?1;")?
            .query_row(params![key], |row| row.get(0))
    }
}

pub mod budget {
    use super::*;

    // Gets the row id for the budget, creating a new row if one does not already exist.
    pub fn get_or_create(
        conn: &Connection,
        budget_summary: &BudgetSummary,
    ) -> Result<i64, rusqlite::Error> {
        let uuid = budget_summary.id.hyphenated().to_string();
        let mut stmt = conn.prepare("SELECT id FROM budget WHERE uuid = ?")?;
        match stmt
            .query_row([&uuid], |row| row.get(0))
            .optional()
            .unwrap()
        {
            Some(id) => Ok(id),
            None => {
                conn.execute(
                    "INSERT INTO budget(uuid, name) VALUES (?1, ?2);",
                    params![uuid, budget_summary.name],
                )?;
                Ok(conn.last_insert_rowid())
            }
        }
    }
}

pub mod account {
    use super::*;

    pub fn create_if_not_exists(
        conn: &Connection,
        budget_id: i64,
        accounts: &Vec<Account>,
    ) -> Result<(), rusqlite::Error> {
        for acc in accounts.iter() {
            let uuid = acc.id.hyphenated().to_string();
            conn.execute(
                "INSERT INTO account(budget_id, uuid, name) VALUES (?1, ?2, ?3) \
                ON CONFLICT(uuid) DO UPDATE SET name=?3;",
                params![budget_id, uuid, acc.name],
            )?;
        }
        Ok(())
    }
}
