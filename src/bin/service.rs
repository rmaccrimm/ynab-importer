use notify_debouncer_full::notify::{
    event::{CreateKind, Event},
    EventKind::Create,
    RecursiveMode,
};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use rusqlite::Connection;
use std::{error::Error, fs, path::PathBuf};
use std::{sync::mpsc::channel, time::Duration};
use uuid::Uuid;
use ynab_api::models::{
    NewTransaction, TransactionClearedStatus, TransactionDetail, TransactionResponseData,
};
use ynab_importer::ofx::parse;
use ynab_importer::{db::config, ofx::OfxTransaction};

fn load_transactions(path: &PathBuf) -> Vec<OfxTransaction> {
    let path = &event.paths[0];
    match fs::read_to_string(path) {
        Ok(content) => match parse(&content) {
            Ok(transactions) => {
                return transactions;
            }
            Err(err) => {
                println!(
                    "Failed to parse file {}: {}",
                    &path.display(),
                    err.to_string()
                )
            }
        },
        Err(err) => {
            println!(
                "Failed to read file {}: {}",
                path.display(),
                err.to_string()
            );
        }
    }
    vec![]
}

fn get_account_id_from_path(conn: &Connection, path: &PathBuf) -> Option<UUId> {
    todo!();
}

fn create_transactions(conn: &Connection, event: &DebouncedEvent) {
    if event.paths.len() == 0 {
        println!("No paths provided with create event.");
        return;
    }
    let path = &event.paths[0];

    let account_id = match get_account_id_from_path(conn, path) {
        Some(id) => id,
        None => {
            println!(
                "Import failed. Could not determine account from path {}",
                path.display()
            );
            return;
        }
    };

    let new_transactions = load_transactions(path).iter().map(|t| NewTransaction {
        account_id: account_id,
        date: Some(t.date_posted.to_string()),
        amount: Some(t.amount),
        payee_id: None,
        payee_name: Some(t.name),
        category_id: None,
        memo: Some(t.memo),
        cleared: Some(TransactionClearedStatus::Cleared),
        approved: None,
        flag_color: None,
        subtransactions: None,
        import_id: None,
    });
    todo!();
}

// fn get_budget_and_account_for_path
fn event_handler(conn: &Connection, event: &DebouncedEvent) {
    match event.kind {
        Create(CreateKind::File) | Create(CreateKind::Any) => create_transactions(&conn, &event),
        _ => println!("Ignored event {:#?}", event),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::open("./db.sqlite3")?;

    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx)?;

    let watch_dir = config::get(&conn, config::TRANSACTION_DIR)?;
    debouncer.watch(&watch_dir, RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(events) => {
                for event in events.iter() {
                    event_handler(&conn, event);
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
    Ok(())
}
