//! Command-line interface for SmartFox measurements.

mod model;
mod mqtt;
mod smartfox;

use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use display_full_error::DisplayFullErrorExt;
use thiserror::Error;
use tracing::{debug, error, info, instrument};
use tracing_subscriber::EnvFilter;

use crate::{
    model::{EnergySnapshot, EnergyTotals},
    mqtt::{MqttConfig, MqttCredentials, MqttPublisher},
    smartfox::SmartFoxClient,
};

/// Parses command-line arguments.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Defines supported commands.
#[derive(Subcommand)]
enum Commands {
    /// Fetches SmartFox values and prints the normalized model.
    Dump {
        /// SmartFox configuration.
        #[command(flatten)]
        smartfox: SmartFoxArgs,
    },
    /// Publishes Home Assistant MQTT discovery and one state update.
    Export {
        /// SmartFox configuration.
        #[command(flatten)]
        smartfox: SmartFoxArgs,
        /// MQTT configuration.
        #[command(flatten)]
        mqtt: MqttArgs,
    },
    /// Publishes Home Assistant MQTT discovery and continuous state updates.
    Run {
        /// SmartFox configuration.
        #[command(flatten)]
        smartfox: SmartFoxArgs,
        /// MQTT configuration.
        #[command(flatten)]
        mqtt: MqttArgs,
        /// Refresh interval between SmartFox updates.
        #[arg(
            long,
            env = "HAFOX_REFRESH_INTERVAL",
            default_value = "30s",
            value_parser = parse_duration
        )]
        refresh_interval: Duration,
    },
}

/// Describes SmartFox command-line configuration.
#[derive(Args)]
struct SmartFoxArgs {
    /// SmartFox web interface base URL.
    #[arg(long, env = "HAFOX_SMARTFOX_URL", default_value = "http://smartfox")]
    smartfox_url: String,
}

/// Describes MQTT command-line configuration.
#[derive(Args)]
struct MqttArgs {
    /// MQTT broker host name or address.
    #[arg(long, env = "HAFOX_MQTT_HOST", default_value = "localhost")]
    mqtt_host: String,
    /// MQTT broker TCP port.
    #[arg(long, env = "HAFOX_MQTT_PORT", default_value_t = 1883)]
    mqtt_port: u16,
    /// MQTT client identifier.
    #[arg(long, env = "HAFOX_MQTT_CLIENT_ID", default_value = "hafox")]
    mqtt_client_id: String,
    /// MQTT user name.
    #[arg(long, env = "HAFOX_MQTT_USERNAME")]
    mqtt_username: Option<String>,
    /// MQTT password.
    #[arg(long, env = "HAFOX_MQTT_PASSWORD")]
    mqtt_password: Option<String>,
    /// Home Assistant discovery topic prefix.
    #[arg(
        long,
        env = "HAFOX_MQTT_DISCOVERY_PREFIX",
        default_value = "homeassistant"
    )]
    mqtt_discovery_prefix: String,
    /// hafox state topic prefix.
    #[arg(long, env = "HAFOX_MQTT_TOPIC_PREFIX", default_value = "hafox")]
    mqtt_topic_prefix: String,
}

impl MqttArgs {
    /// Converts command-line arguments into MQTT configuration.
    fn into_config(self) -> Result<MqttConfig, Error> {
        let credentials = match (self.mqtt_username, self.mqtt_password) {
            (Some(username), Some(password)) => Some(MqttCredentials { username, password }),
            (None, None) => None,
            (Some(_), None) => return Err(Error::MissingMqttPassword),
            (None, Some(_)) => return Err(Error::MissingMqttUsername),
        };

        Ok(MqttConfig {
            host: self.mqtt_host,
            port: self.mqtt_port,
            client_id: self.mqtt_client_id,
            credentials,
            discovery_prefix: self.mqtt_discovery_prefix,
            topic_prefix: self.mqtt_topic_prefix,
        })
    }
}

/// Runs the command-line application.
#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    run().await.map_err(|error| {
        error!(error = %error.display_full(), "command failed");
        error.into()
    })
}

/// Runs the selected command.
async fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Dump { smartfox } => dump(&smartfox.smartfox_url).await,
        Commands::Export { smartfox, mqtt } => {
            export_once(&smartfox.smartfox_url, mqtt.into_config()?).await
        }
        Commands::Run {
            smartfox,
            mqtt,
            refresh_interval,
        } => {
            run_continuously(
                &smartfox.smartfox_url,
                mqtt.into_config()?,
                refresh_interval,
            )
            .await
        }
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

/// Fetches SmartFox values and prints a normalized snapshot.
#[instrument(skip_all, fields(smartfox_url = %smartfox_url), err)]
async fn dump(smartfox_url: &str) -> Result<(), Error> {
    let client = SmartFoxClient::new(smartfox_url).map_err(|source| Error::SmartFox { source })?;
    let snapshot = fetch_snapshot(&client).await?;

    println!("{snapshot:#?}");
    Ok(())
}

/// Publishes one MQTT discovery and state update.
#[instrument(skip_all, fields(smartfox_url = %smartfox_url), err)]
async fn export_once(smartfox_url: &str, mqtt_config: MqttConfig) -> Result<(), Error> {
    let client = SmartFoxClient::new(smartfox_url).map_err(|source| Error::SmartFox { source })?;
    let snapshot = fetch_snapshot(&client).await?;
    validate_lifetime_counters(&snapshot, None)?;
    let publisher = MqttPublisher::connect(&mqtt_config)
        .await
        .map_err(mqtt_error)?;

    publisher
        .publish_discovery(&snapshot)
        .await
        .map_err(mqtt_error)?;
    publisher
        .publish_state(&snapshot)
        .await
        .map_err(mqtt_error)?;
    publisher
        .publish_availability(true)
        .await
        .map_err(mqtt_error)?;
    publisher.flush().await.map_err(mqtt_error)?;

    info!("exported MQTT discovery and state");
    Ok(())
}

/// Runs continuous MQTT state updates.
#[instrument(skip_all, fields(smartfox_url = %smartfox_url), err)]
async fn run_continuously(
    smartfox_url: &str,
    mqtt_config: MqttConfig,
    refresh_interval: Duration,
) -> Result<(), Error> {
    let client = SmartFoxClient::new(smartfox_url).map_err(|source| Error::SmartFox { source })?;
    let mut state = RunState::default();

    loop {
        match update_mqtt(&client, &mqtt_config, &mut state).await {
            Ok(discovery_published) => {
                info!(discovery_published, "updated MQTT state");
            }
            Err(error) => {
                if matches!(&error, Error::Mqtt { .. }) {
                    state.publisher = None;
                }
                error!(error = %error.display_full(), "update failed; retrying");
            }
        }

        tokio::time::sleep(refresh_interval).await;
    }
}

/// Fetches and normalizes one SmartFox snapshot.
#[instrument(skip(client))]
async fn fetch_snapshot(client: &SmartFoxClient) -> Result<EnergySnapshot, Error> {
    let values = client
        .fetch_values()
        .await
        .map_err(|source| Error::SmartFox { source })?;
    EnergySnapshot::from_smartfox_values(&values).map_err(|source| Error::Model { source })
}

/// Publishes one continuous update iteration.
#[instrument(skip_all)]
async fn update_mqtt(
    smartfox: &SmartFoxClient,
    mqtt_config: &MqttConfig,
    state: &mut RunState,
) -> Result<bool, Error> {
    let snapshot = match fetch_snapshot(smartfox).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            publish_offline(state).await;
            return Err(error);
        }
    };
    let retained_energy = mqtt::read_retained_energy(mqtt_config)
        .await
        .map_err(mqtt_error)?;
    if let Err(error) = validate_lifetime_counters(&snapshot, retained_energy.as_ref()) {
        publish_offline(state).await;
        return Err(error);
    }

    if state.publisher.is_none() {
        state.publisher = Some(
            MqttPublisher::connect(mqtt_config)
                .await
                .map_err(mqtt_error)?,
        );
    }
    let publisher = state
        .publisher
        .as_ref()
        .expect("publisher should be present after connecting");

    let discovery_published = if state.discovery_published {
        false
    } else {
        publisher
            .publish_discovery(&snapshot)
            .await
            .map_err(mqtt_error)?;
        state.discovery_published = true;
        true
    };

    publisher
        .publish_state(&snapshot)
        .await
        .map_err(mqtt_error)?;
    publisher
        .publish_availability(true)
        .await
        .map_err(mqtt_error)?;
    Ok(discovery_published)
}

/// Validates lifetime counters before publishing them to Home Assistant.
fn validate_lifetime_counters(
    snapshot: &EnergySnapshot,
    previous: Option<&EnergyTotals>,
) -> Result<(), Error> {
    if snapshot.energy.solar_production.watt_hours <= 0 {
        return Err(Error::InvalidLifetimeCounter {
            field: "solar_production",
            value: snapshot.energy.solar_production.watt_hours,
        });
    }

    if let Some(previous) = previous {
        validate_lifetime_counter(
            "grid_import",
            previous.grid_import.watt_hours,
            snapshot.energy.grid_import.watt_hours,
        )?;
        validate_lifetime_counter(
            "grid_export",
            previous.grid_export.watt_hours,
            snapshot.energy.grid_export.watt_hours,
        )?;
        validate_lifetime_counter(
            "solar_production",
            previous.solar_production.watt_hours,
            snapshot.energy.solar_production.watt_hours,
        )?;
    }

    Ok(())
}

/// Validates that one lifetime counter did not decrease.
fn validate_lifetime_counter(
    field: &'static str,
    previous: i64,
    current: i64,
) -> Result<(), Error> {
    if current < previous {
        return Err(Error::DecreasedLifetimeCounter {
            field,
            previous,
            current,
        });
    }

    Ok(())
}

/// Publishes offline availability when a publisher is available.
async fn publish_offline(state: &RunState) {
    if let Some(publisher) = &state.publisher
        && let Err(error) = publisher.publish_availability(false).await
    {
        debug!(%error, "failed to publish MQTT offline availability");
    }
}

/// Converts an MQTT module error into an application error.
fn mqtt_error(source: mqtt::Error) -> Error {
    Error::Mqtt {
        source: Box::new(source),
    }
}

/// Parses a human-readable duration.
fn parse_duration(value: &str) -> Result<Duration, humantime::DurationError> {
    humantime::parse_duration(value)
}

/// Stores continuous run state.
#[derive(Default)]
struct RunState {
    /// MQTT publisher, connected after the first successful snapshot.
    publisher: Option<MqttPublisher>,
    /// Whether Home Assistant discovery was already published.
    discovery_published: bool,
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
    /// Indicates an MQTT publishing failure.
    #[error("MQTT operation failed")]
    Mqtt {
        /// MQTT error source.
        #[source]
        source: Box<mqtt::Error>,
    },
    /// Indicates that a username is needed for the configured password.
    #[error("MQTT username is required when MQTT password is configured")]
    MissingMqttUsername,
    /// Indicates that a lifetime counter is not safe to publish.
    #[error("lifetime counter `{field}` has invalid value `{value}`")]
    InvalidLifetimeCounter {
        /// Counter field name.
        field: &'static str,
        /// Counter value.
        value: i64,
    },
    /// Indicates that a lifetime counter moved backwards.
    #[error("lifetime counter `{field}` decreased from {previous} to {current}")]
    DecreasedLifetimeCounter {
        /// Counter field name.
        field: &'static str,
        /// Previous published value.
        previous: i64,
        /// Current candidate value.
        current: i64,
    },
    /// Indicates that a password is needed for the configured username.
    #[error("MQTT password is required when MQTT username is configured")]
    MissingMqttPassword,
}
