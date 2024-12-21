use rusqlite::types::{FromSql, FromSqlError};
use rusqlite::{self, ToSql};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;
use ynab_api::models::Account;
use ynab_api::models::BudgetSummary;

use anyhow::Result;

// Wrapper around Uuid that can be saved/loaded from sqlite db automatically
pub struct DbUuid(pub Uuid);

impl Into<Uuid> for DbUuid {
    fn into(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for DbUuid {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl FromSql for DbUuid {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).and_then(|as_string| {
            Uuid::parse_str(&as_string)
                .map(DbUuid::from)
                .map_err(|err| FromSqlError::Other(Box::new(err)))
        })
    }
}

impl ToSql for DbUuid {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.0.hyphenated().to_string().into())
    }
}

pub mod config {
    use std::{ffi::OsString, path::PathBuf};

    use super::*;

    pub const USER_ID: &str = "user_id";
    pub const ACCESS_TOKEN: &str = "access_token";
    pub const TRANSACTION_DIR: &str = "transaction_dir";

    // Set the key value pair in configuration table
    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<usize> {
        let id = conn.execute(
            "INSERT INTO configuration(key, value) VALUES (?1, ?2) \
            ON CONFLICT(key) DO UPDATE SET value=?2;",
            params![key, value],
        )?;
        Ok(id)
    }

    // Get a value from configuration table
    pub fn get(conn: &Connection, key: &str) -> Result<String> {
        let s = conn
            .prepare("SELECT value FROM configuration WHERE key=?1;")?
            .query_row(params![key], |row| row.get(0))?;
        Ok(s)
    }

    pub fn set_transaction_dir(conn: &Connection, path: &PathBuf) -> Result<usize> {
        set(
            conn,
            TRANSACTION_DIR,
            &serde_json::to_string(path.as_os_str())?,
        )
    }

    pub fn get_transaction_dir(conn: &Connection) -> Result<PathBuf> {
        let ser = get(conn, TRANSACTION_DIR)?;
        let path = PathBuf::from(serde_json::from_str::<OsString>(&ser)?);
        Ok(path)
    }
}

pub mod budget {
    use uuid::Uuid;

    use super::*;

    // Gets the row id for the budget, creating a new row if one does not already exist.
    pub fn get_or_create(conn: &Connection, budget_summary: &BudgetSummary) -> Result<i64> {
        let uuid = DbUuid(budget_summary.id);
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

    pub fn get_id(conn: &Connection, budget_name: &str) -> Result<i64> {
        let mut stmt = conn.prepare("SELECT id FROM budget WHERE name = ?")?;
        let result: i64 = stmt.query_row([&budget_name], |row| row.get(0))?;
        Ok(result)
    }

    pub fn get_uuid(conn: &Connection, budget_name: &str) -> Result<Uuid> {
        let mut stmt = conn.prepare("SELECT uuid FROM budget WHERE name = ?")?;
        let result: String = stmt.query_row([&budget_name], |row| row.get(0))?;
        let uuid = Uuid::parse_str(&result)?;
        Ok(uuid)
    }
}

pub mod account {
    use uuid::Uuid;

    use super::*;

    pub fn create_if_not_exists(
        conn: &Connection,
        budget_id: i64,
        accounts: &Vec<Account>,
    ) -> Result<()> {
        for acc in accounts.iter() {
            let uuid = DbUuid(acc.id);
            conn.execute(
                "INSERT INTO account(budget_id, uuid, name) VALUES (?1, ?2, ?3) \
                ON CONFLICT(uuid) DO UPDATE SET name=?3;",
                params![budget_id, uuid, acc.name],
            )?;
        }
        Ok(())
    }

    pub struct AccountRow {
        pub id: i64,
        pub budget_id: i64,
        pub uuid: Uuid,
        pub name: String,
    }

    pub fn get(conn: &Connection, budget_id: i64, account_name: &str) -> Result<AccountRow> {
        let mut stmt = conn.prepare(
            "SELECT id, budget_id, uuid, name FROM account WHERE name = ? AND budget_id = ?",
        )?;
        let result: AccountRow = stmt.query_row(params![&account_name, &budget_id], |row| {
            Ok(AccountRow {
                id: row.get(0)?,
                budget_id: row.get(1)?,
                uuid: row.get::<usize, DbUuid>(2)?.into(),
                name: row.get(3)?,
            })
        })?;
        Ok(result)
    }
}

pub mod transaction {
    use chrono::NaiveDate;

    use super::*;

    pub fn exists(
        conn: &Connection,
        account_id: i64,
        amount_milli: i64,
        date_posted: NaiveDate,
    ) -> Result<bool> {
        let mut stmt = conn.prepare(
            "SELECT id FROM transaction_import \
            WHERE account_id = ? AND amount = ? AND date_posted = ?",
        )?;
        let result: Option<i32> = stmt
            .query_row(
                params![account_id, amount_milli, date_posted.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.is_some())
    }

    pub fn create_if_not_exists(
        conn: &Connection,
        account_id: i64,
        amount_milli: i64,
        date_posted: NaiveDate,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO transaction_import(account_id, amount, date_posted) VALUES (?, ?, ?) \
            ON CONFLICT(amount, date_posted, account_id) DO NOTHING;",
            params![account_id, amount_milli, date_posted.to_string()],
        )?;
        Ok(())
    }
}
