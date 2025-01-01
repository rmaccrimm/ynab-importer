#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use refinery::embed_migrations;
use rusqlite::Connection;
use ynab_importer::ui::ConfigApp;

embed_migrations!();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = Connection::open("./db.sqlite3")?;
    migrations::runner().run(&mut conn)?;
    drop(conn);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 280.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "YNAB Importer",
        options,
        Box::new(|cc| Ok(Box::new(ConfigApp::new(cc)))),
    )?;
    Ok(())
}
