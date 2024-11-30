use clap::Parser;
use refinery::embed_migrations;
use rusqlite::Connection;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio;
use ynab_api::apis::configuration::Configuration;
use ynab_api::apis::{accounts_api::get_accounts, budgets_api::get_budgets};
use ynab_importer::db::{account, budget, config};

use rusqlite;

use std::io;
use std::io::Write;

use ynab_api::models::Account;
use ynab_api::models::BudgetSummary;

use serde_json;

embed_migrations!();

#[derive(Parser, Debug)]
struct Args {
    // Your YNAB user id (email address)
    #[arg(short, long)]
    user_id: String,

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

pub fn create_dir_if_not_exists(path: &PathBuf) -> io::Result<()> {
    match fs::create_dir(&path) {
        Ok(()) => {
            println!("Created {}", path.display());
            return Ok(());
        }
        Err(err) => match err.kind() {
            io::ErrorKind::AlreadyExists => {
                println!("{} already exists", path.display());
                return Ok(());
            }
            _ => Err(err),
        },
    }
}

pub fn create_directories(
    transaction_dir: &PathBuf,
    budget: &BudgetSummary,
    accounts: &Vec<Account>,
) -> io::Result<()> {
    let mut path = transaction_dir.clone();
    path.push(&budget.name);
    create_dir_if_not_exists(&path)?;

    for acc in accounts.iter() {
        path.push(&acc.name);
        create_dir_if_not_exists(&path)?;
        path.pop();
    }
    Ok(())
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

    let mut config = Configuration::new();
    config.bearer_access_token = Some(token.clone());

    let budget_response = get_budgets(&config, None).await?;
    let budgets = budget_response.data.budgets;
    if budgets.len() == 0 {
        return Err("Account has no budgets".into());
    }

    let mut budget = &budgets[0];
    if budgets.len() > 1 {
        budget = prompt_budget(&budgets);
    }

    let budget_uuid = budget.id.hyphenated().to_string();
    let accounts = get_accounts(&config, &budget_uuid, None)
        .await?
        .data
        .accounts;

    create_directories(&transaction_dir, budget, &accounts)?;

    let tx = conn.transaction()?;

    let budget_id = budget::get_or_create(&tx, budget)?;
    account::create_if_not_exists(&tx, budget_id, &accounts)?;
    config::set(&tx, config::USER_ID, &args.user_id)?;
    config::set(
        &tx,
        config::TRANSACTION_DIR,
        &serde_json::to_string(transaction_dir.as_os_str())?,
    )?;
    config::set(&tx, config::ACCESS_TOKEN, &token)?;

    tx.commit()?;
    Ok(())
}
