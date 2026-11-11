use std::{net::SocketAddr, path::PathBuf};

use recipe_shelf::{core::App, web};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database = PathBuf::from(
        std::env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/recipes.sqlite".into()),
    );
    if let Some(parent) = database.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let app = App::open(&database)?;
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()?;
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!(
        "Recipe Shelf listening on http://{}",
        listener.local_addr()?
    );
    topcoat::serve(listener, web::router(app)).await?;
    Ok(())
}
