mod api;
mod cli;
mod db;
mod models;
mod services;
mod utils;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "oddsforge")]
#[command(about = "A sports analytics platform for prediction markets")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the API server
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Fetch sports data
    Fetch {
        #[arg(short, long)]
        sport: String,
    },
    /// Generate predictions for upcoming matches
    Predict,
    /// Query team statistics
    Team {
        #[arg(short, long)]
        name: String,
    },
    /// Initialize the database
    InitDb,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Load environment variables
    dotenv::dotenv().ok();
    
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { port }) => {
            tracing::info!("Starting OddsForge API server on port {}", port);
            api::serve(port).await?;
        }
        Some(Commands::Fetch { sport }) => {
            tracing::info!("Fetching data for sport: {}", sport);
            cli::fetch_data(&sport).await?;
        }
        Some(Commands::Predict) => {
            tracing::info!("Generating predictions...");
            cli::generate_predictions().await?;
        }
        Some(Commands::Team { name }) => {
            tracing::info!("Querying team: {}", name);
            cli::query_team(&name).await?;
        }
        Some(Commands::InitDb) => {
            tracing::info!("Initializing database...");
            db::init_database().await?;
        }
        None => {
            // Default to serving
            tracing::info!("Starting OddsForge API server on port 3000");
            api::serve(3000).await?;
        }
    }

    Ok(())
}