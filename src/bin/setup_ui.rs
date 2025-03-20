#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui::{self, IconData, ViewportBuilder};
use image::EncodableLayout;
use refinery::embed_migrations;
use std::sync::Arc;
use std::{fs, path::Path};
use ynab_importer::{db::get_sqlite_conn, ui::ConfigApp};

embed_migrations!();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    {
        let mut conn = get_sqlite_conn()?;
        migrations::runner().run(&mut conn)?;
    }

    let icon = image::open(Path::new("./img/Yi.png"))?.to_rgba8();
    let (icon_width, icon_height) = icon.dimensions();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 280.0])
            .with_drag_and_drop(true)
            .with_icon(IconData {
                rgba: icon.into_raw(),
                width: icon_width,
                height: icon_height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "YNAB Importer",
        options,
        Box::new(|cc| Ok(Box::new(ConfigApp::new(cc)))),
    )?;
    Ok(())
}
