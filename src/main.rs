//! Command-line interface for SmartFox measurements.

mod model;
mod smartfox;

use std::error::Error as StdError;

use anyhow::Result;
use clap::{Parser, Subcommand};
use thiserror::Error;
use tracing::{error, info, instrument};
use tracing_subscriber::EnvFilter;

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
async fn main() -> Result<()> {
    init_tracing();

    run().await.map_err(|error| {
        report_error(&error);
        error.into()
    })
}

/// Runs the selected command.
async fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Fetch { smartfox_url } => fetch(&smartfox_url).await,
    }
}

/// Configures structured logging.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("hafox=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Logs an error with its source chain.
fn report_error(error: &Error) {
    error!(%error, "command failed");

    let mut source = StdError::source(error);
    while let Some(error) = source {
        error!(%error, "caused by");
        source = error.source();
    }
}

/// Fetches SmartFox values and prints a normalized snapshot.
#[instrument(skip_all, fields(smartfox_url = %smartfox_url), err)]
async fn fetch(smartfox_url: &str) -> Result<(), Error> {
    info!("fetching SmartFox values");
    let client = SmartFoxClient::new(smartfox_url).map_err(|source| Error::SmartFox { source })?;
    let values = client
        .fetch_values()
        .await
        .map_err(|source| Error::SmartFox { source })?;

    info!("normalizing SmartFox values");
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
