use super::error::ImportError;
use super::{
    db::{account, budget, config},
    ofx::load_transactions,
};
use crate::db::transaction;
use crate::ofx::OfxTransaction;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use notify_debouncer_full::notify::{event::CreateKind, EventKind::Create};
use notify_debouncer_full::DebouncedEvent;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;
use ynab_api::apis::configuration::Configuration;
use ynab_api::apis::transactions_api::create_transaction;
use ynab_api::models::{NewTransaction, PostTransactionsWrapper, TransactionClearedStatus};

fn milli_dollar_amount(amount: f64) -> i64 {
    (amount * 1000.0).round() as i64
}

impl From<OfxTransaction> for NewTransaction {
    fn from(value: OfxTransaction) -> Self {
        NewTransaction {
            account_id: None,
            date: Some(value.date_posted.to_string()),
            amount: Some(milli_dollar_amount(value.amount)),
            payee_id: None,
            payee_name: Some(value.name.clone()),
            category_id: None,
            memo: Some(value.memo.clone()),
            cleared: Some(TransactionClearedStatus::Cleared),
            approved: None,
            flag_color: None,
            subtransactions: None,
            import_id: None,
        }
    }
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

#[derive(Hash, Clone, PartialEq, Eq)]
struct TransactionKey {
    date: NaiveDate,
    amount_millis: i64,
    occurrence: usize,
}

impl TransactionKey {
    fn get_id(&self) -> String {
        let mut s = String::new();
        write!(
            s,
            "ynab_importer:{}:{}:{}",
            self.date, self.amount_millis, self.occurrence
        )
        .unwrap();
        s
    }
}

pub struct EventHandler {
    db_conn: Connection,
    api_config: Configuration,
    max_retries: usize,
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
                max_retries: 10,
            }
        })
    }

    async fn create_transactions_with_retry(&self, event: &DebouncedEvent) -> Result<()> {
        if event.paths.len() == 0 {
            return Err(ImportError::NoPathError.into());
        }
        let path = &event.paths[0];
        let base_dir = config::get_transaction_dir(&self.db_conn)?;
        let (budget_name, account_name) = get_budget_and_account_from_path(&base_dir, path)?;

        let budget_id = budget::get_id(&self.db_conn, &budget_name)
            .with_context(|| format!("failed to load budget id for {}", budget_name))?;
        let account = account::get(&self.db_conn, budget_id, &account_name)
            .with_context(|| format!("failed to load account for {}", account_name))?;

        let mut transaction_map = HashMap::new();
        let mut new_transactions = Vec::new();

        for t in load_transactions(path)?.into_iter() {
            let amount_millis = milli_dollar_amount(t.amount);
            let mut key = TransactionKey {
                date: t.date_posted,
                amount_millis,
                occurrence: 1,
            };
            if transaction::exists(&self.db_conn, account.id, amount_millis, key.date)? {
                println!(
                    "Transaction with amount ${} on {} already imported. Skipping",
                    t.amount, key.date
                );
                continue;
            }
            let mut import_id = key.get_id();
            while transaction_map.contains_key(&import_id) {
                key.occurrence += 1;
                import_id = key.get_id();
            }

            let mut new_transaction = NewTransaction::from(t);
            new_transaction.account_id = Some(account.uuid);
            new_transaction.import_id = Some(Some(import_id.clone()));

            transaction_map.insert(import_id, (key, new_transaction.clone()));
            new_transactions.push(new_transaction);
        }
        let budget_uuid = budget::get_uuid(&self.db_conn, &budget_name)?;

        let mut retry = 0;
        loop {
            let resp = create_transaction(
                &self.api_config,
                &budget_uuid.hyphenated().to_string(),
                PostTransactionsWrapper {
                    transaction: None,
                    transactions: Some(new_transactions.clone()),
                },
            )
            .await?;
            println!("{:?}", resp);
            new_transactions.clear();

            match resp.data.transactions {
                Some(transactions) => {
                    for saved_transaction in transactions.iter() {
                        let (key, _) = transaction_map
                            .get(&saved_transaction.import_id.clone().unwrap().unwrap())
                            .unwrap();

                        transaction::create_if_not_exists(
                            &self.db_conn,
                            account.id,
                            key.amount_millis,
                            key.date,
                        )?;
                    }
                }
                None => (),
            }

            match resp.data.duplicate_import_ids {
                None => {
                    break;
                }
                Some(ids) => {
                    if retry == self.max_retries {
                        return Err(anyhow!(
                            "One or more transactions were not succesfully imported, {:#?}",
                            ids
                        ));
                    }
                    for import_id in ids {
                        let (key, transaction) = transaction_map.get(&import_id).unwrap();
                        let mut new_key = key.clone();
                        new_key.occurrence += 1;
                        let import_id = new_key.get_id();

                        let new_transaction = NewTransaction {
                            import_id: Some(Some(import_id.clone())),
                            ..transaction.clone()
                        };
                        transaction_map.insert(import_id, (new_key, new_transaction.clone()));
                        new_transactions.push(new_transaction);
                    }
                    retry += 1;
                }
            }
        }
        Ok(())
    }

    pub async fn handle(&self, event: &DebouncedEvent) -> Result<()> {
        match event.kind {
            Create(CreateKind::File) | Create(CreateKind::Any) => {
                self.create_transactions_with_retry(event).await
            }
            _ => {
                println!("Ignored event {:?}", event);
                Ok(())
            }
        }
    }
}
