use clap::Parser;
use refinery::embed_migrations;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use tokio;
use ynab_api::apis::configuration::Configuration;
use ynab_api::apis::{accounts_api::get_accounts, budgets_api::get_budgets};
use ynab_api::models::Account;
use ynab_api::models::BudgetSummary;

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
    transaction_dir: PathBuf,
}

pub mod config {
    use rusqlite;
    use rusqlite::{params, Connection};

    pub const USER_ID: &str = "user_id";
    pub const ACCESS_TOKEN: &str = "access_token";
    pub const TRANSACTION_DIR: &str = "transaction_dir";

    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<usize, rusqlite::Error> {
        conn.execute(
            "INSERT INTO configuration(key, value) VALUES (?1, ?2) \
            ON CONFLICT(key) DO UPDATE SET value=?2;",
            params![key, value],
        )
    }
}
// Gets the row id for the budget, creating a new row if one does not already exist.
pub fn insert_budget(
    conn: &Connection,
    budget_summary: &BudgetSummary,
) -> Result<i64, rusqlite::Error> {
    let uuid = budget_summary.id.hyphenated().to_string();
    let mut stmt = conn.prepare("SELECT id FROM budget WHERE uuid = ?")?;
    match stmt
        .query_row([&uuid], |row| row.get(0))
        .optional()
        .unwrap()
    {
        Some(id) => Ok(id),
        None => {
            conn.execute(
                "INSERT INTO budget(uuid, name) VALUES (?1, ?2);",
                params![uuid, budget_summary.name],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }
}

fn insert_accounts(
    conn: &Connection,
    budget_id: i64,
    accounts: &Vec<Account>,
) -> Result<(), rusqlite::Error> {
    for acc in accounts.iter() {
        let uuid = acc.id.hyphenated().to_string();
        conn.execute(
            "INSERT INTO account(budget_id, uuid, name) VALUES (?1, ?2, ?3) \
            ON CONFLICT(uuid) DO UPDATE SET name=?3;",
            params![budget_id, uuid, acc.name],
        )?;
    }
    Ok(())
}

fn read_prompt_int(options: &Vec<usize>) -> usize {
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

fn prompt_budget(budgets: &Vec<BudgetSummary>) -> &BudgetSummary {
    println!("The following budgets were found for this account:");
    for (i, b) in budgets.iter().enumerate() {
        println!("[{}]: {}", i + 1, b.name);
    }
    print!("Enter the account you like to use [1-{}]: ", budgets.len());
    let sel = read_prompt_int(&(1..=budgets.len()).collect());
    &budgets[sel - 1]
}

fn create_dir_if_not_exists(path: &PathBuf) -> io::Result<()> {
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

fn create_directories(
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

    if !fs::exists(args.transaction_dir.as_path())? {
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

    create_directories(&args.transaction_dir, budget, &accounts)?;

    let tx = conn.transaction()?;

    let budget_id = insert_budget(&tx, budget)?;
    insert_accounts(&tx, budget_id, &accounts)?;
    config::set(&tx, config::USER_ID, &args.user_id)?;
    config::set(
        &tx,
        config::TRANSACTION_DIR,
        &args
            .transaction_dir
            .canonicalize()?
            .to_str()
            .expect("Bad directory path provided"),
    )?;
    config::set(&tx, config::ACCESS_TOKEN, &token)?;

    tx.commit()?;
    Ok(())
}
