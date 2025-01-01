use anyhow::Result;
use notify_debouncer_full::new_debouncer;
use notify_debouncer_full::notify::RecursiveMode;
use refinery::embed_migrations;
use std::{sync::mpsc::channel, time::Duration};
use ynab_importer::{
    db::{config, get_sqlite_conn},
    event::EventHandler,
};

embed_migrations!();

#[tokio::main]
async fn main() -> Result<()> {
    let mut db_conn = get_sqlite_conn()?;
    migrations::runner().run(&mut db_conn)?;

    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx)?;
    let watch_dir = config::get_transaction_dir(&db_conn)?;
    let event_handler = EventHandler::new(db_conn)?;
    // sync_transactions(&event_handler.db_conn, &event_handler.api_config);

    debouncer.watch(&watch_dir, RecursiveMode::Recursive)?;
    for res in rx {
        match res {
            Ok(events) => {
                for event in events.iter() {
                    if let Err(err) = event_handler.handle(event).await {
                        println!("{:?}", err);
                    };
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
    Ok(())
}
