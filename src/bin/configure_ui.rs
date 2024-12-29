#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::egui::{self, FontId, RichText, Spinner, Ui};
use notify_debouncer_full::notify::Config;
use std::fs;
use std::io::Read;
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};
use tokio::runtime::Runtime;
use ynab_api::{self, models::BudgetSummary};
use ynab_api::{
    apis::{
        accounts_api::get_accounts,
        budgets_api::{get_budgets, GetBudgetsError},
        configuration::Configuration,
    },
    models::{Account, BudgetSummaryResponse},
};
use ynab_importer::db::budget;

#[tokio::main]
async fn main() -> eframe::Result {
    let rt = Runtime::new().expect("Unable to create Runtime");

    // Enter the runtime so that `tokio::spawn` is available immediately.
    let _enter = rt.enter();

    // Execute the runtime in its own thread.
    // The future doesn't have to do anything. In this example, it just sleeps forever.
    std::thread::spawn(move || {
        rt.block_on(async {
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        })
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 240.0]) // wide enough for the drag-drop overlay text
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Native file dialogs and drag-and-drop files",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp::new()))),
    )
}

enum LoadingState<T> {
    Loading,
    Success(T),
    Failed(String),
}

struct BudgetSelect {
    state: LoadingState<Vec<BudgetSummary>>,
    selected: Vec<bool>,
}

impl BudgetSelect {
    fn new() -> Self {
        BudgetSelect {
            state: LoadingState::Loading,
            selected: Vec::new(),
        }
    }

    fn set_budgets(&mut self, budgets: Vec<BudgetSummary>) {
        self.selected = vec![false; budgets.len()];
        self.state = LoadingState::Success(budgets);
    }

    fn set_error(&mut self, msg: String) {
        self.state = LoadingState::Failed(msg);
    }

    fn draw_ui(&mut self, ui: &mut Ui) {
        match &self.state {
            LoadingState::Loading => {
                ui.label("Loading budgets...");
            }
            LoadingState::Success(budgets) => {
                for (i, b) in budgets.iter().enumerate() {
                    ui.checkbox(&mut self.selected[i], b.name.clone());
                }
            }
            LoadingState::Failed(msg) => {
                ui.label(msg);
            }
        }
    }
}

struct MyApp {
    dropped_files: Vec<egui::DroppedFile>,
    picked_path: Option<String>,
    api_config: Option<Configuration>,
    tx: Sender<GetBudgetsResponse>,
    rx: Receiver<GetBudgetsResponse>,
    budget_select: Option<BudgetSelect>,
    transaction_dir: String,
}

impl MyApp {
    fn new() -> Self {
        let (tx, rx) = channel();
        MyApp {
            dropped_files: vec![],
            picked_path: None,
            api_config: None,
            tx,
            rx,
            budget_select: None,
            transaction_dir: String::new(),
        }
    }

    fn load_access_token(&mut self, path: String) -> Result<(), String> {
        let mut pat_file = fs::File::open(&path).map_err(|err| err.to_string())?;
        let mut token = String::new();
        pat_file
            .read_to_string(&mut token)
            .map_err(|err| err.to_string())?;
        let mut api_config = Configuration::new();
        api_config.bearer_access_token = Some(token);
        self.api_config = Some(api_config);
        Ok(())
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Personal Access Token").font(FontId::proportional(20.0)));
                ui.label("Drag-and-drop file here or");

                if ui.button("Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        self.picked_path = Some(path.display().to_string());
                    }
                }
            });

            if let Some(picked_path) = &self.picked_path {
                ui.horizontal(|ui| {
                    ui.label("Picked file:");
                    ui.monospace(picked_path);
                });
            }
            ui.add(Spinner::new().size(50.0));

            // Show dropped files (if any):
            if !self.dropped_files.is_empty() {
                ui.group(|ui| {
                    ui.label("Dropped files:");

                    for file in &self.dropped_files {
                        let mut info = if let Some(path) = &file.path {
                            path.display().to_string()
                        } else if !file.name.is_empty() {
                            file.name.clone()
                        } else {
                            "???".to_owned()
                        };

                        let mut additional_info = vec![];
                        if !file.mime.is_empty() {
                            additional_info.push(format!("type: {}", file.mime));
                        }
                        if let Some(bytes) = &file.bytes {
                            additional_info.push(format!("{} bytes", bytes.len()));
                        }
                        if !additional_info.is_empty() {
                            info += &format!(" ({})", additional_info.join(", "));
                        }

                        ui.label(info);
                    }
                });
            }
            let submit = ui.button("Submit");
            if submit.clicked() {
                // Create the budget select in initial loading state
                self.budget_select = Some(BudgetSelect::new());
                // make_budget_request(ctx.clone(), self.tx.clone(), self.api_config.clone());
            }

            if let Some(bsel) = &mut self.budget_select {
                ui.label("User id: placeholder");
                ui.label("Select budget(s):");
                bsel.draw_ui(ui);
                ui.horizontal(|ui| {
                    ui.label("Monitored folder location");
                    ui.text_edit_singleline(&mut self.transaction_dir);
                    if ui.button("Browse").clicked() {
                        todo!();
                    }
                });
                if ui.button("Create Directories").clicked() {
                    todo!();
                }
            }

            if let Ok(result) = self.rx.try_recv() {
                match result {
                    Ok(resp) => {
                        self.budget_select
                            .as_mut()
                            .map(|bsel| bsel.set_budgets(resp.data.budgets));
                    }
                    Err(err) => {
                        self.budget_select
                            .as_mut()
                            .map(|bsel| bsel.set_error(err.to_string()));
                    }
                }
            }
        });

        preview_files_being_dropped(ctx);

        // Collect dropped files:
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                self.dropped_files.clone_from(&i.raw.dropped_files);
            }
        });
    }
}

/// Preview hovering files:
fn preview_files_being_dropped(ctx: &egui::Context) {
    use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};
    use std::fmt::Write as _;

    if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
        let text = ctx.input(|i| {
            let mut text = "Dropping files:\n".to_owned();
            for file in &i.raw.hovered_files {
                if let Some(path) = &file.path {
                    write!(text, "\n{}", path.display()).ok();
                } else if !file.mime.is_empty() {
                    write!(text, "\n{}", file.mime).ok();
                } else {
                    text += "\n???";
                }
            }
            text
        });

        let painter =
            ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

        let screen_rect = ctx.screen_rect();
        painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(192));
        painter.text(
            screen_rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Heading.resolve(&ctx.style()),
            Color32::WHITE,
        );
    }
}
