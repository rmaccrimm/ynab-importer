use anyhow::{Context, Result};
use notify_debouncer_full::notify::{event::CreateKind, EventKind::Create, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use rusqlite::Connection;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::{sync::mpsc::channel, time::Duration};
use ynab_api::models::{NewTransaction, TransactionClearedStatus};
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

fn create_transactions(conn: &Connection, event: &DebouncedEvent) -> Result<()> {
    if event.paths.len() == 0 {
        return Err(ImportError::NoPathError.into());
    }
    let path = &event.paths[0];
    let base_dir = config::get_transaction_dir(conn)?;
    let (budget_name, account_name) = get_budget_and_account_from_path(&base_dir, path)?;

    let budget_id =
        budget::get_id(conn, &budget_name).with_context(|| "failed to load budget id")?;
    let account_id = account::get_uuid(conn, budget_id, &account_name)
        .with_context(|| "failed to load account")?;

    let new_transactions = load_transactions(path)?
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
        });
    println!("{:#?}", new_transactions);
    Ok(())
}

// fn get_budget_and_account_for_path
fn event_handler(conn: &Connection, event: &DebouncedEvent) -> Result<()> {
    match event.kind {
        Create(CreateKind::File) | Create(CreateKind::Any) => create_transactions(&conn, &event),
        _ => {
            println!("Ignored event {:?}", event);
            Ok(())
        }
    }
}

fn main() -> Result<()> {
    let conn = Connection::open("./db.sqlite3")?;

    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx)?;

    let watch_dir = config::get_transaction_dir(&conn)?;
    debouncer.watch(&watch_dir, RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(events) => {
                for event in events.iter() {
                    match event_handler(&conn, event) {
                        Err(err) => {
                            println!("{}", err.to_string());
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
