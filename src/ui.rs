use eframe;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender};
use ynab_api::{
    apis::{budgets_api::get_budgets, budgets_api::GetBudgetsError, configuration::Configuration},
    models::{BudgetSummary, BudgetSummaryResponse, BudgetSummaryResponseData, UserResponse},
};

pub struct DragAndDropView {
    dropped_files: Vec<egui::DroppedFile>,
    picked_path: Option<String>,
}

impl eframe::App for DragAndDropView {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        todo!();
    }
}

pub struct LoadingView {}

impl eframe::App for LoadingView {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        todo!();
    }
}

pub struct BudgetSelectView {
    budgets: Vec<BudgetSummary>,
    selected: Vec<bool>,
}

impl BudgetSelectView {
    async fn init(config: &Configuration) -> Result<Self, String> {
        let budgets = get_budgets(&config, Some(true))
            .await
            .map_err(|err| err.to_string())
            .map(|resp| resp.data.budgets)?;

        Ok(BudgetSelectView {
            selected: vec![false; budgets.len()],
            budgets,
        })
    }
}

impl eframe::App for BudgetSelectView {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        todo!();
    }
}

// May expand to include other API responses if needed
type ApiResponse = Result<BudgetSelectView, String>;

// type GetBudgetsResponse = Result<BudgetSummaryResponse, ynab_api::apis::Error<GetBudgetsError>>;

// type GetUserResponse = Result<UserResponse, ynab_api::apis::Error<GetUserError>>;

pub struct ConfigApp {
    current_view: Box<dyn eframe::App>,
    api_config: Option<Configuration>,
    tx: Sender<ApiResponse>,
    rx: Receiver<ApiResponse>,
}

fn make_budget_request(ctx: egui::Context, tx: Sender<ApiResponse>, api_config: Configuration) {
    tokio::spawn(async move {
        tx.send(BudgetSelectView::init(&api_config).await)
            .expect("Channel was closed");
        ctx.request_repaint();
    });
}
