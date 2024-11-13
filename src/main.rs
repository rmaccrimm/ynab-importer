use refinery::embed_migrations;
use rusqlite::Connection;
use tokio;

embed_migrations!();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = Connection::open("./db.sqlite3")?;
    migrations::runner().run(&mut conn)?;
    Ok(())
}
