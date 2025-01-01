use clap::Parser;
use refinery::embed_migrations;
use rusqlite;
use rusqlite::Connection;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use tokio;
use ynab_api::apis::configuration::Configuration;
use ynab_api::apis::{accounts_api::get_accounts, budgets_api::get_budgets};
use ynab_api::models::Account;
use ynab_api::models::BudgetSummary;
use ynab_importer::db::{account, budget, config};
use ynab_importer::setup::run_setup;

use serde_json;

embed_migrations!();

#[derive(Parser, Debug)]
struct Args {
    // Path to your personal access token
    #[arg(short, long)]
    access_token: PathBuf,

    // Folder to monitor for transaction exports
    #[arg(short, long)]
    transaction_dir: OsString,
}

pub fn read_prompt_int(options: &Vec<usize>) -> usize {
    loop {
        io::stdout().flush().expect("stdout flush failed");
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        match input.trim().parse::<usize>() {
            Ok(n) => {
                if !options.contains(&n) {
                    print!("Invalid selection. Please try again: ");
                    continue;
                }
                return n;
            }
            Err(error) => {
                println!("Error: {error}");
                print!("Please try again: ");
            }
        }
    }
}

pub fn prompt_budget(budgets: &Vec<BudgetSummary>) -> &BudgetSummary {
    println!("The following budgets were found for this account:");
    for (i, b) in budgets.iter().enumerate() {
        println!("[{}]: {}", i + 1, b.name);
    }
    print!("Enter the account you like to use [1-{}]: ", budgets.len());
    let sel = read_prompt_int(&(1..=budgets.len()).collect());
    &budgets[sel - 1]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let transaction_dir = PathBuf::from(args.transaction_dir).canonicalize()?;

    if !fs::exists(&transaction_dir)? {
        return Err("Directory does not exist".into());
    }

    let mut conn = Connection::open("./db.sqlite3")?;
    migrations::runner().run(&mut conn)?;

    let mut pat_file = fs::File::open(&args.access_token)?;
    let mut token = String::new();
    pat_file.read_to_string(&mut token)?;

    let mut api_config = Configuration::new();
    api_config.bearer_access_token = Some(token.clone());

    let budget_response = get_budgets(&api_config, Some(true)).await?;
    let budgets = budget_response.data.budgets;
    if budgets.len() == 0 {
        return Err("Account has no budgets".into());
    }

    let mut budget = budgets[0].clone();
    if budgets.len() > 1 {
        budget = prompt_budget(&budgets).clone();
    }

    let (sx, rx) = mpsc::channel();
    tokio::task::spawn_blocking(move || {
        run_setup(conn, &api_config, &transaction_dir, vec![budget], sx)
    });
    while let Ok(msg) = rx.recv() {
        println!("{}", msg);
    }

    Ok(())
}