use anyhow::Result;
use image::EncodableLayout;
use notify_debouncer_full::new_debouncer;
use notify_debouncer_full::notify::RecursiveMode;
use refinery::embed_migrations;
use std::{path::Path, sync::mpsc::channel, time::Duration};
use tray_icon::{
    menu::{Menu, MenuItem, Submenu},
    Icon, TrayIconBuilder, TrayIconEvent,
};
use ynab_importer::{
    db::{config, get_sqlite_conn},
    event::EventHandler,
};

embed_migrations!();

#[tokio::main]
async fn main() -> Result<()> {
    // let icon = image::open(Path::new("./img/Yi.png"))?.to_rgba8();
    // let (icon_width, icon_height) = icon.dimensions();

    // let tray_menu = Menu::with_id("menu_id");
    // tray_menu.append(&Submenu::new("Exit", true))?;

    // let _tray_icon = TrayIconBuilder::new()
    //     .with_tooltip("YNAB-importer")
    //     .with_icon(Icon::from_rgba(
    //         icon.as_bytes().to_vec(),
    //         icon_width,
    //         icon_height,
    //     )?)
    //     .with_menu(Box::new(tray_menu))
    //     .build()?;

    // if let Ok(event) = TrayIconEvent::receiver().recv() {
    //     println!("{:?}", event);
    // }
    let mut db_conn = get_sqlite_conn()?;
    migrations::runner().run(&mut db_conn)?;

    // File system event channel
    let (tx_fs, rx_fs) = channel();

    // Tray menu event channel
    // let (tx_tray, rx_tray) = channel();

    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx_fs)?;
    let watch_dir = config::get_transaction_dir(&db_conn)?;
    println!("{}", watch_dir.display());
    let event_handler = EventHandler::new(db_conn)?;
    // sync_transactions(&event_handler.db_conn, &event_handler.api_config);

    debouncer.watch(&watch_dir, RecursiveMode::Recursive)?;
    for res in rx_fs {
        match res {
            Ok(events) => {
                for event in events {
                    if let Err(err) = event_handler.handle(&event).await {
                        println!("{:?}", err);
                    };
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        }
    }
    Ok(())
}
