use super::db::account;
use crate::db::account::AccountRow;
use crate::db::transaction::TransactionRow;
use crate::db::{budget, config, transaction};
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use tokio::task::JoinSet;
use ynab_api::apis::{configuration::Configuration, transactions_api::get_transactions_by_account};
use ynab_api::models::{Account, BudgetSummary};

fn create_dir_if_not_exists(path: &PathBuf) -> io::Result<()> {
    match fs::create_dir(&path) {
        Ok(()) => {
            println!("Created {}", path.display());
            return Ok(());
        }
        Err(err) => match err.kind() {
            io::ErrorKind::AlreadyExists => {
                println!("{} already exists", path.display());
                return Ok(());
            }
            _ => Err(err),
        },
    }
}

pub fn create_directories(
    transaction_dir: &PathBuf,
    budget: &BudgetSummary,
    accounts: &Vec<Account>,
) -> io::Result<()> {
    let mut path = transaction_dir.clone();
    path.push(&budget.name);
    create_dir_if_not_exists(&path)?;

    for acc in accounts.iter() {
        path.push(&acc.name);
        create_dir_if_not_exists(&path)?;
        path.pop();
    }
    Ok(())
}

async fn make_transactions_request(
    api_config: Configuration,
    budget_uuids: HashMap<i64, String>,
    accounts: Vec<AccountRow>,
    tx: Sender<String>,
) -> Result<Vec<TransactionRow>> {
    let mut set: JoinSet<Result<Vec<TransactionRow>>> = JoinSet::new();
    for acc in accounts {
        let budget_uuid = budget_uuids
            .get(&acc.id)
            .ok_or(anyhow!("Missing account id {}", acc.id))?
            .clone();
        let api_config = api_config.clone();
        let acc = acc.clone();
        let tx = tx.clone();

        set.spawn(async move {
            let response = get_transactions_by_account(
                &api_config,
                &budget_uuid,
                &acc.uuid.hyphenated().to_string(),
                None,
                None,
                None,
            )
            .await?;
            let transactions: Vec<TransactionRow> = response
                .data
                .transactions
                .into_iter()
                .map(|t| TransactionRow::new(t.amount, t.date, acc.id))
                .collect();
            let msg = String::from(format!(
                "Storing {} transactions for account {}",
                transactions.len(),
                acc.name
            ));
            tx.send(msg).expect("Channel was closed");
            Ok(transactions)
        });
    }
    let joined: Vec<Result<Vec<TransactionRow>>> = set.join_all().await;
    let transactions = joined
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<TransactionRow>>();
    Ok(transactions)
}

pub fn sync_transactions(
    mut conn: Connection,
    api_config: &Configuration,
    tx_msg: Sender<String>,
) -> Result<()> {
    let accounts = account::get_all(&conn)?;

    let mut budget_uuids = HashMap::new();
    for acc in accounts.iter() {
        let budget = budget::get(&conn, acc.budget_id)?;
        budget_uuids.insert(acc.id, budget.uuid.hyphenated().to_string());
    }

    let (tx_trans, rx) = mpsc::channel();
    let api_config = api_config.clone();
    tokio::spawn(async move {
        let result = make_transactions_request(api_config, budget_uuids, accounts, tx_msg).await;
        tx_trans.send(result).expect("Channel was closed");
    });

    let tx = conn.transaction()?;
    loop {
        match rx.recv() {
            Ok(res) => {
                for t in res? {
                    transaction::create_if_not_exists(&tx, t)?;
                }
            }
            Err(_) => {
                break;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn run_setup(
    // SQLite connection
    mut conn: Connection,

    // API configuration object (with bearer access token)
    api_config: &Configuration,

    // Path to create subdirectories in
    transaction_dir: &PathBuf,

    // Budget objects from get_budgets API, with accounts loaded
    budgets: Vec<BudgetSummary>,

    // Channel to send status messages over
    tx_msg: Sender<String>,
) -> Result<()> {
    if !fs::exists(&transaction_dir)? {
        return Err(anyhow!("Directory does not exist"));
    }
    let tx = conn.transaction()?;
    for budget in budgets {
        let accounts = budget.accounts.clone().unwrap_or(Vec::new());
        create_directories(&transaction_dir, &budget, &accounts)?;
        tx_msg
            .send(format!("Created directories for {}", &budget.name.clone()).into())
            .expect("Channel was closed");

        let budget_id = budget::get_or_create(&tx, &budget)?;
        account::create_if_not_exists(&tx, budget_id, &accounts)?;
        config::set_transaction_dir(&tx, &transaction_dir)?;
        config::set(
            &tx,
            config::TRANSACTION_DIR,
            &serde_json::to_string(transaction_dir.as_os_str())?,
        )?;
        config::set(
            &tx,
            config::ACCESS_TOKEN,
            &api_config.bearer_access_token.clone().unwrap(),
        )?;
    }
    tx.commit()?;
    sync_transactions(conn, &api_config, tx_msg.clone())?;
    tx_msg
        .send("Setup Complete".into())
        .expect("Channel was closed");
    Ok(())
}
