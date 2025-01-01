use anyhow::Result;
use eframe::egui::{self, Context, FontId, Spinner, Theme};
use eframe::{self, egui::RichText};
use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};
use std::env::current_dir;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, channel, Receiver, Sender};
use ynab_api::{
    apis::{budgets_api::get_budgets, configuration::Configuration},
    models::BudgetSummary,
};

use crate::db::get_sqlite_conn;
use crate::setup::run_setup;

type View = Box<dyn eframe::App + Send>;

/*
Main application state machine. All the actual rendering and state transition logic is implented by
the states (Views) themselves.
 */
pub struct ConfigApp {
    current_view: View,
    rx: Receiver<View>,
}

impl ConfigApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(Theme::Dark);
        cc.egui_ctx.set_zoom_factor(1.5);
        let (tx, rx) = channel();
        Self {
            current_view: Box::new(DragAndDropFileView::new(tx.clone())),
            rx,
        }
    }
}

impl eframe::App for ConfigApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.current_view.update(ctx, frame);
        if let Ok(next_state) = self.rx.try_recv() {
            self.current_view = next_state;
        }
    }
}

// Initial state, asks the user to provide a file containing the personal access token
struct DragAndDropFileView {
    tx: Sender<View>,
    picked_path: Option<PathBuf>,
    error: Option<String>,
}

impl DragAndDropFileView {
    fn new(tx: Sender<View>) -> Self {
        Self {
            tx,
            picked_path: None,
            error: None,
        }
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.picked_path = Some(path);
        }
    }

    fn check_dropped_files(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = &i.raw.dropped_files[0].path {
                    self.picked_path = Some(path.clone());
                }
            }
        });
    }

    fn preview_files_being_dropped(&self, ctx: &egui::Context) {
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let text = ctx.input(|i| {
                let mut text = String::new();
                for file in &i.raw.hovered_files {
                    if let Some(path) = &file.path {
                        write!(text, "{}", path.display()).ok();
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

    // Transitions to the loading state and also initiates the tokio task to hit the YNAB api, which
    // will initiate transition to the budget select state on completion.
    fn next_state(&self, ctx: Context) -> Result<()> {
        if let Some(path) = &self.picked_path {
            let mut pat_file = fs::File::open(&path)?;
            let mut token = String::new();
            pat_file.read_to_string(&mut token)?;

            let mut api_config = Configuration::new();
            api_config.bearer_access_token = Some(token);

            self.tx
                .send(Box::new(LoadingView()))
                .expect("Channel was closed");

            let tx = self.tx.clone();
            tokio::spawn(async move {
                let next = match MonitoredFolderFormView::init(api_config).await {
                    // Go to form view
                    Ok(form_view) => Box::new(form_view) as View,
                    // Go back to initial state and show error message
                    Err(err) => Box::new(DragAndDropFileView {
                        tx: tx.clone(),
                        picked_path: None,
                        error: Some(err.to_string()),
                    }),
                };
                tx.send(next).expect("Channel was closed");
                ctx.request_repaint();
            });
        }
        Ok(())
    }
}

impl eframe::App for DragAndDropFileView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label("");
                ui.label(RichText::new("Personal Access Token").font(FontId::proportional(20.0)));
                ui.label("Drag-and-drop file here or");

                if ui.button("Browse").clicked() {
                    self.open_file_dialog();
                }
            });

            self.preview_files_being_dropped(ctx);
            self.check_dropped_files(ctx);
            if let Err(err) = self.next_state(ctx.clone()) {
                self.error = Some(err.to_string());
            }
        });

        egui::TopBottomPanel::bottom("error_pannel")
            .show_separator_line(false)
            .show(ctx, |ui| {
                if let Some(msg) = &self.error {
                    ui.label(RichText::new(msg).color(Color32::LIGHT_RED));
                }
            });
    }
}

// A simple loading screen
struct LoadingView();

impl eframe::App for LoadingView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Loading budgets").font(FontId::proportional(20.0)));
                    ui.add(Spinner::new().size(40.0));
                });
            });
        });
    }
}

// Final state. Form for selecting the folder to monitor and which budgets to create subfolders for.
struct MonitoredFolderFormView {
    api_config: Configuration,
    budgets: Vec<BudgetSummary>,
    selected: Vec<bool>,
    transaction_dir: String,
    setup_running: bool,
    error: Option<String>,
    log_msg: Option<String>,
    rx_msg: Option<Receiver<String>>,
    tx_err: Sender<String>,
    rx_err: Receiver<String>,
}

impl MonitoredFolderFormView {
    async fn init(api_config: Configuration) -> Result<Self> {
        let budgets = get_budgets(&api_config, Some(true))
            .await
            .map(|resp| resp.data.budgets)?;

        let (tx_err, rx_err) = mpsc::channel();

        Ok(MonitoredFolderFormView {
            api_config,
            selected: vec![false; budgets.len()],
            budgets,
            transaction_dir: current_dir()
                .map(|b| b.display().to_string())
                .unwrap_or(String::new()),
            setup_running: false,
            error: None,
            log_msg: None,
            rx_msg: None,
            tx_err,
            rx_err,
        })
    }

    fn start_setup(&mut self) -> Result<()> {
        self.setup_running = true;
        self.error = None;

        let (tx, rx) = mpsc::channel();
        self.rx_msg = Some(rx);

        let conn = get_sqlite_conn()?;
        let config = self.api_config.clone();
        let path = PathBuf::from(&self.transaction_dir);
        let budgets = self.budgets.clone();

        let tx_err = self.tx_err.clone();
        tokio::task::spawn_blocking(move || {
            let result = run_setup(conn, &config, &path, budgets, tx);
            if let Err(err) = result {
                tx_err.send(err.to_string()).expect("Channel was closed");
            }
        });
        Ok(())
    }

    fn poll_messages(&mut self) {
        if let Ok(err) = self.rx_err.try_recv() {
            self.error = Some(err.to_string());
        }
        // rx_msg is None until setup is started
        if let Some(rx) = &self.rx_msg {
            match rx.try_recv() {
                Ok(msg) => {
                    self.log_msg = Some(msg);
                }
                Err(err) => {
                    // Sender was dropped meaning setup task has completed
                    if err == mpsc::TryRecvError::Disconnected {
                        self.rx_msg = None;
                        self.setup_running = false;
                    }
                }
            }
        }
    }
}

impl eframe::App for MonitoredFolderFormView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Monitored folder location:");
            ui.end_row();

            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.transaction_dir);
                if ui.button("Browse").clicked() {
                    todo!();
                }
            });
            ui.add_space(10.0);

            ui.label("Select the budget(s) to create subfolders for:");
            for (i, b) in self.budgets.iter().enumerate() {
                ui.checkbox(&mut self.selected[i], b.name.clone());
                ui.end_row();
            }
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                if self.setup_running {
                    ui.spinner();
                } else {
                    if ui.button("Start Setup").clicked() {
                        if let Err(err) = self.start_setup() {
                            self.error = Some(err.to_string())
                        }
                    }
                }
                if let Some(msg) = &self.log_msg {
                    ui.label(msg);
                }
            });
        });

        egui::TopBottomPanel::bottom("error_pannel")
            .show_separator_line(false)
            .show(ctx, |ui| {
                if let Some(msg) = &self.error {
                    ui.label(RichText::new(msg).color(Color32::LIGHT_RED));
                }
            });

        self.poll_messages();
    }
}
