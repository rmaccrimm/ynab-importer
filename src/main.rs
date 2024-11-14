use clap::Parser;
use refinery::embed_migrations;
use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use tokio;

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

    pub fn insert(conn: &Connection, key: &str, value: &str) -> Result<usize, rusqlite::Error> {
        conn.execute(
            "INSERT INTO configuration(key, value) VALUES (?1, ?2) \
            ON CONFLICT(key) DO UPDATE SET value=?2;",
            params![key, value],
        )
    }
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

    let tx = conn.transaction()?;

    config::insert(&tx, config::USER_ID, &args.user_id)?;
    config::insert(
        &tx,
        config::TRANSACTION_DIR,
        &args
            .transaction_dir
            .canonicalize()?
            .to_str()
            .expect("Bad directory path provided"),
    )?;
    config::insert(&tx, config::ACCESS_TOKEN, &token)?;

    tx.commit()?;
    Ok(())
}
