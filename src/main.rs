//! Command-line interface for SmartFox measurements.

mod model;
mod smartfox;

use clap::{Parser, Subcommand};
use thiserror::Error;

use crate::{model::EnergySnapshot, smartfox::SmartFoxClient};

/// Parses command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Defines supported commands.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Fetches SmartFox values and prints the normalized model.
    Fetch {
        /// SmartFox web interface base URL.
        #[arg(long, env = "HAFOX_SMARTFOX_URL", default_value = "http://smartfox")]
        smartfox_url: String,
    },
}

/// Runs the command-line application.
#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch { smartfox_url } => fetch(&smartfox_url).await,
    }
}

/// Fetches SmartFox values and prints a normalized snapshot.
async fn fetch(smartfox_url: &str) -> Result<(), Error> {
    let client = SmartFoxClient::new(smartfox_url).map_err(|source| Error::SmartFox { source })?;
    let values = client
        .fetch_values()
        .await
        .map_err(|source| Error::SmartFox { source })?;
    let snapshot =
        EnergySnapshot::from_smartfox_values(&values).map_err(|source| Error::Model { source })?;

    println!("{snapshot:#?}");
    Ok(())
}

/// Reports application failures.
#[derive(Debug, Error)]
enum Error {
    /// Indicates a SmartFox client failure.
    #[error("SmartFox operation failed")]
    SmartFox {
        /// SmartFox error source.
        #[source]
        source: smartfox::Error,
    },
    /// Indicates a model conversion failure.
    #[error("SmartFox values could not be normalized")]
    Model {
        /// Model conversion error source.
        #[source]
        source: model::Error,
    },
}
