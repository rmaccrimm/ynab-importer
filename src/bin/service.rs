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

fn get_budget_name_from_path(
    basedir_path: &PathBuf,
    path: &PathBuf,
) -> Result<OsString, ImportError> {
    let mut display_path = String::new();
    write!(&mut display_path, "{}", path.display()).unwrap();

    let mut display_basedir = String::new();
    write!(&mut display_basedir, "{}", path.display()).unwrap();

    let mut path = path.clone();
    while path.parent() != None {
        let parent = match path.parent() {
            Some(p) => p,
            None => {
                break;
            }
        };
        if parent == basedir_path {
            match path.file_name() {
                Some(p) => {
                    return Ok(p.to_owned());
                }
                None => {
                    break;
                }
            }
        }
        if !path.pop() {
            break;
        }
    }
    Err(ImportError::PathParsingError(display_path))
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

    let account_id = get_budget_name_from_path(&base_dir, path)?;
    println!("{:?}", account_id);

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

    #[test]
    fn test_path_ops() {
        let mut path = PathBuf::from(r"C:\Users\Roddy\Repos\ynab_importer\downloads\AppTestingBudget\Test MasterCard\madeup.QFX")
            .canonicalize()
            .unwrap();
        println!("{:?}", path);
        let download_dir = PathBuf::from(r"\\?\C:\Users\Roddy\Repos\ynab_importer\downloads");
        let base_dir = download_dir.file_name().unwrap();

        let mut subdir = None;
        while path.parent() != None {
            let parent = path.parent().unwrap().file_name().unwrap();
            println!("{:?}", parent);
            if parent == base_dir {
                subdir = Some(path.file_name());
                break;
            }
            if !path.pop() {
                break;
            }
        }
        println!("{:?}", subdir);
    }
}
