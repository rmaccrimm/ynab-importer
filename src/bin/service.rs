use notify_debouncer_full::notify::{
    event::{CreateKind, Event},
    EventKind::Create,
    RecursiveMode,
};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use rusqlite::Connection;
use std::{
    error::Error,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};
use std::{ffi::OsStr, fmt::Write};
use std::{sync::mpsc::channel, time::Duration};
use thiserror::Error;
use uuid::Uuid;
use ynab_api::models::{
    NewTransaction, TransactionClearedStatus, TransactionDetail, TransactionResponseData,
};
use ynab_importer::ofx::parse;
use ynab_importer::{db::config, ofx::OfxTransaction};

#[derive(Error, Debug)]
pub enum ImportError {
    #[error("something went wrong parsing the event path '{0}'")]
    PathParsingError(String),

    #[error("failed to parse QFX file")]
    FileParsingError(#[from] sgmlish::Error),

    #[error("no paths provided with event")]
    NoPathError,

    #[error("missing configuration {0}")]
    MissingConfigurationError(String),

    #[error("something went wrong talking to the database")]
    DBError(#[from] rusqlite::Error),

    #[error("failed to load file")]
    FileIOError(#[from] io::Error),
}

fn load_transactions(path: &PathBuf) -> Vec<OfxTransaction> {
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

fn get_budget_and_account_from_path(
    basedir_path: &PathBuf,
    path: &PathBuf,
) -> Result<(String, String), ImportError> {
    let mut display_path = String::new();
    write!(&mut display_path, "{}", path.display()).unwrap();

    let mut display_basedir = String::new();
    write!(&mut display_basedir, "{}", path.display()).unwrap();

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
        return Err(ImportError::PathParsingError(display_path));
    }
    Ok((budget_name.unwrap().into(), account_name.unwrap().into()))
}

fn create_transactions(conn: &Connection, event: &DebouncedEvent) -> Result<(), ImportError> {
    if event.paths.len() == 0 {
        return Err(ImportError::NoPathError);
    }
    let path = &event.paths[0];

    let base_dir = match config::get(&conn, config::TRANSACTION_DIR) {
        Ok(dir) => Ok(dir),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(ImportError::MissingConfigurationError(
            config::TRANSACTION_DIR.into(),
        )),
        Err(err) => Err(err.into()),
    }?;
    let base_dir = PathBuf::from(base_dir).canonicalize()?;
    println!("{:?}", base_dir);

    let (budget_name, account_name) = get_budget_and_account_from_path(&base_dir, path)?;
    println!("{:?}, {:?}", budget_name, account_name);

    return Ok(());

    let new_transactions = load_transactions(path).iter().map(|t| NewTransaction {
        account_id: todo!(),
        date: Some(t.date_posted.to_string()),
        amount: todo!(),
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
fn event_handler(conn: &Connection, event: &DebouncedEvent) -> Result<(), ImportError> {
    match event.kind {
        Create(CreateKind::File) | Create(CreateKind::Any) => create_transactions(&conn, &event),
        _ => {
            println!("Ignored event {:#?}", event);
            Ok(())
        }
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
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_path_ops() {
        let path = PathBuf::from(r"C:\Users\Roddy\Repos\ynab_importer\downloads\AppTestingBudget\Test MasterCard\Chequing.QFX")
            .canonicalize()
            .unwrap();

        println!(
            "{}",
            serde_json::to_string(&path.as_os_str().to_os_string()).unwrap()
        );
        println!(
            "{:?}",
            serde_json::from_str::<OsString>(
                &serde_json::to_string(&path.as_os_str().to_os_string()).unwrap()
            )
            .unwrap()
        )
    }
}
