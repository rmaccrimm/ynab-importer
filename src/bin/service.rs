use notify_debouncer_full::notify::{
    event::{CreateKind, Event},
    EventKind::{Access, Create, Modify, Other, Remove},
    RecursiveMode,
};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, DebouncedEvent};
use rusqlite::Connection;
use std::{error::Error, fs};
use std::{
    sync::mpsc::{channel, Receiver},
    time::Duration,
};
use ynab_importer::db::{account, budget, config};

fn create_transactions(conn: &Connection, event: &Event) {
    if event.paths.len() == 0 {
        println!("No paths provided with create event.");
        return;
    }
    match fs::read_to_string(&event.paths[0]) {
        Ok(content) => {
            todo!();
            // let qif = parse(&content, "%d/%m/%Y").expect("Failed to parse");
            // println!("{:#?}", qif);
        }
        Err(_) => println!("Failed to read file"),
    }
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
