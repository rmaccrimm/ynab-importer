use eframe::egui::scroll_area::State;
use eframe::egui::{self, Context, FontId, Spinner};
use eframe::{self, egui::RichText};
use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};
use refinery::config;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use ynab_api::{
    apis::{budgets_api::get_budgets, configuration::Configuration},
    models::BudgetSummary,
};

type View = Box<dyn eframe::App + Send>;

struct DragAndDropView {
    tx: Sender<View>,
    picked_path: Option<PathBuf>,
    error: Option<String>,
}

impl DragAndDropView {
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
    fn next_state(&self, ctx: Context) -> Result<(), String> {
        if let Some(path) = &self.picked_path {
            let mut pat_file = fs::File::open(&path).map_err(|err| err.to_string())?;
            let mut token = String::new();
            pat_file
                .read_to_string(&mut token)
                .map_err(|err| err.to_string())?;

            let mut api_config = Configuration::new();
            api_config.bearer_access_token = Some(token);

            self.tx
                .send(Box::new(LoadingView()))
                .expect("Channel was closed");

            let tx = self.tx.clone();
            tokio::spawn(async move {
                let next = match BudgetSelectView::init(api_config, tx.clone()).await {
                    Ok(budget_select) => Box::new(budget_select) as View,
                    Err(msg) => Box::new(DragAndDropView {
                        tx: tx.clone(),
                        picked_path: None,
                        error: Some(msg),
                    }),
                };
                tx.send(next).expect("Channel was closed");
                ctx.request_repaint();
            });
        }
        Ok(())
    }
}

impl eframe::App for DragAndDropView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Personal Access Token").font(FontId::proportional(20.0)));
                ui.label("Drag-and-drop file here or");

                if ui.button("Browse").clicked() {
                    self.open_file_dialog();
                }
            });

            self.preview_files_being_dropped(ctx);
            self.check_dropped_files(ctx);
            if let Err(msg) = self.next_state(ctx.clone()) {
                self.error = Some(msg);
            }
            if let Some(msg) = &self.error {
                ui.label(msg);
            }
        });
    }
}

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

struct BudgetSelectView {
    api_config: Configuration,
    tx: Sender<View>,
    budgets: Vec<BudgetSummary>,
    selected: Vec<bool>,
    error: Option<String>,
    transaction_dir: String,
}

impl BudgetSelectView {
    async fn init(api_config: Configuration, tx: Sender<View>) -> Result<Self, String> {
        let budgets = get_budgets(&api_config, Some(true))
            .await
            .map_err(|err| err.to_string())
            .map(|resp| resp.data.budgets)?;

        Ok(BudgetSelectView {
            api_config,
            tx,
            selected: vec![false; budgets.len()],
            budgets,
            error: None,
            transaction_dir: String::new(),
        })
    }
}

impl eframe::App for BudgetSelectView {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Select the budget(s) to configure folders for:");
            for (i, b) in self.budgets.iter().enumerate() {
                ui.checkbox(&mut self.selected[i], b.name.clone());
            }
            ui.label("Monitored folder location:");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.transaction_dir);
                if ui.button("Browse").clicked() {
                    todo!();
                }
            });
            if ui.button("Create Directories").clicked() {
                todo!();
            }
        });
    }
}

pub struct ConfigApp {
    current_view: View,
    rx: Receiver<View>,
}

impl ConfigApp {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            current_view: Box::new(DragAndDropView::new(tx.clone())),
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
