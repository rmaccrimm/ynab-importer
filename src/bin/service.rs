use anyhow::{Context, Result};
use notify_debouncer_full::notify::Config;
use notify_debouncer_full::notify::{event::CreateKind, EventKind::Create, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use rusqlite::Connection;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::{sync::mpsc::channel, time::Duration};
use tokio::main;
use ynab_api::apis::configuration::Configuration;
use ynab_api::apis::transactions_api::create_transaction;
use ynab_api::models::{NewTransaction, PostTransactionsWrapper, TransactionClearedStatus};
use ynab_importer::error::ImportError;
use ynab_importer::ofx::parse;
use ynab_importer::{
    db::{account, budget, config},
    ofx::OfxTransaction,
};

fn load_transactions(path: &PathBuf) -> Result<Vec<OfxTransaction>> {
    let content = fs::read_to_string(path)?;
    let ts = parse(&content).map_err(|err| ImportError::from(err))?;
    Ok(ts)
}

fn get_budget_and_account_from_path(
    basedir_path: &PathBuf,
    path: &PathBuf,
) -> Result<(String, String)> {
    let mut display_path = String::new();
    write!(&mut display_path, "{}", path.display())?;

    let mut display_basedir = String::new();
    write!(&mut display_basedir, "{}", path.display())?;

    let mut new_path = PathBuf::new();
    let mut level_count = 0;
    let mut account_name = None;
    let mut budget_name = None;

    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) => (),
            std::path::Component::RootDir => {
                new_path.push(comp.as_os_str());
                new_path = new_path.canonicalize().unwrap();
            }
            _ => {
                new_path.push(comp.as_os_str());
            }
        }
        if level_count == 0 {
            if &new_path == basedir_path {
                level_count += 1;
            }
        } else if level_count == 1 {
            budget_name = comp.as_os_str().to_str();
            level_count += 1;
        } else {
            account_name = comp.as_os_str().to_str();
            break;
        }
    }
    if budget_name.is_none() || account_name.is_none() {
        return Err(ImportError::PathParsingError(display_path).into());
    }
    Ok((budget_name.unwrap().into(), account_name.unwrap().into()))
}

fn milli_dollar_amount(amount: f64) -> i64 {
    (amount * 1000.0).round() as i64
}

pub struct EventHandler {
    db_conn: Connection,
    api_config: Configuration,
}

impl EventHandler {
    pub fn new(db_conn: Connection) -> Result<Self> {
        let access_token = config::get(&db_conn, config::ACCESS_TOKEN)?;
        let mut api_config = Configuration::new();
        api_config.bearer_access_token = Some(access_token);
        Ok({
            EventHandler {
                db_conn,
                api_config,
            }
        })
    }

    async fn create_transactions(&self, event: &DebouncedEvent) -> Result<()> {
        if event.paths.len() == 0 {
            return Err(ImportError::NoPathError.into());
        }
        let path = &event.paths[0];
        let base_dir = config::get_transaction_dir(&self.db_conn)?;
        let (budget_name, account_name) = get_budget_and_account_from_path(&base_dir, path)?;

        let budget_id = budget::get_id(&self.db_conn, &budget_name)
            .with_context(|| format!("failed to load budget id for {}", budget_name))?;
        let account_id = account::get_uuid(&self.db_conn, budget_id, &account_name)
            .with_context(|| format!("failed to load account for {}", account_name))?;

        let new_transactions: Vec<NewTransaction> = load_transactions(path)?
            .into_iter()
            .map(|t| NewTransaction {
                account_id: Some(account_id),
                date: Some(t.date_posted.to_string()),
                amount: Some(milli_dollar_amount(t.amount)),
                payee_id: None,
                payee_name: Some(t.name.clone()),
                category_id: None,
                memo: Some(t.memo.clone()),
                cleared: Some(TransactionClearedStatus::Cleared),
                approved: None,
                flag_color: None,
                subtransactions: None,
                import_id: None,
            })
            .collect();

        let budget_uuid = budget::get_uuid(&self.db_conn, &budget_name)?;

        let resp = create_transaction(
            &self.api_config,
            &budget_uuid.hyphenated().to_string(),
            PostTransactionsWrapper {
                transaction: None,
                transactions: Some(new_transactions),
            },
        )
        .await?;
        println!("{:#?}", resp.data);

        Ok(())
    }

    pub async fn handle(&self, event: &DebouncedEvent) -> Result<()> {
        match event.kind {
            Create(CreateKind::File) | Create(CreateKind::Any) => {
                self.create_transactions(event).await
            }
            _ => {
                println!("Ignored event {:?}", event);
                Ok(())
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_conn = Connection::open("./db.sqlite3")?;
    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx)?;
    let watch_dir = config::get_transaction_dir(&db_conn)?;
    let event_handler = EventHandler::new(db_conn)?;

    debouncer.watch(&watch_dir, RecursiveMode::Recursive)?;
    for res in rx {
        match res {
            Ok(events) => {
                for event in events.iter() {
                    match event_handler.handle(event).await {
                        Err(err) => {
                            println!("{:?}", err);
                        }
                        _ => (),
                    };
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {}
