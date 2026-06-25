//! Home Assistant MQTT discovery and state publishing.

use std::time::{Duration, SystemTime};

use rumqttc::{AsyncClient, ConnectionError, Event, MqttOptions, Packet, QoS};
use serde::Serialize;
use thiserror::Error;
use tokio::task::JoinHandle;
use tracing::{debug, instrument, trace};

use crate::model::{EnergySnapshot, GridPhase, PhaseMeasurement};

/// Publishes Home Assistant MQTT discovery and state payloads.
#[derive(Debug)]
pub struct MqttPublisher {
    /// MQTT client handle used for outgoing messages.
    client: AsyncClient,
    /// Topic prefix used for hafox state topics.
    topic_prefix: String,
    /// Discovery prefix used by Home Assistant MQTT discovery.
    discovery_prefix: String,
    /// Background MQTT event loop task.
    event_loop: JoinHandle<()>,
}

impl MqttPublisher {
    /// Connects to the configured MQTT broker.
    #[instrument(skip_all, fields(host = %config.host, port = config.port))]
    pub async fn connect(config: &MqttConfig) -> Result<Self, Error> {
        let mut options = MqttOptions::new(&config.client_id, &config.host, config.port);
        options.set_keep_alive(Duration::from_secs(30));
        if let Some(credentials) = &config.credentials {
            options.set_credentials(&credentials.username, &credentials.password);
        }

        let (client, mut event_loop) = AsyncClient::new(options, 100);
        loop {
            match event_loop
                .poll()
                .await
                .map_err(|source| Error::Connection { source })?
            {
                Event::Incoming(Packet::ConnAck(_)) => break,
                event => trace!(?event, "received MQTT event while connecting"),
            }
        }

        let event_loop = tokio::spawn(async move {
            loop {
                if let Err(error) = event_loop.poll().await {
                    debug!(%error, "MQTT event loop error");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });

        Ok(Self {
            client,
            topic_prefix: config.topic_prefix.clone(),
            discovery_prefix: config.discovery_prefix.clone(),
            event_loop,
        })
    }

    /// Publishes retained Home Assistant discovery payloads.
    #[instrument(skip_all, fields(sensor_count))]
    pub async fn publish_discovery(&self, snapshot: &EnergySnapshot) -> Result<(), Error> {
        let sensors = sensor_definitions(snapshot);
        tracing::Span::current().record("sensor_count", sensors.len());

        for sensor in sensors {
            let topic = format!(
                "{}/sensor/{}/config",
                self.discovery_prefix, sensor.unique_id
            );
            let payload = serde_json::to_vec(&sensor.discovery(&self.topic_prefix, snapshot))
                .map_err(|source| Error::Json { source })?;
            self.publish(topic, true, payload).await?;
        }

        Ok(())
    }

    /// Publishes the current retained state payload.
    #[instrument(skip_all)]
    pub async fn publish_state(&self, snapshot: &EnergySnapshot) -> Result<(), Error> {
        let payload = serde_json::to_vec(&StatePayload::from_snapshot(snapshot))
            .map_err(|source| Error::Json { source })?;
        self.publish(self.state_topic(), true, payload).await
    }

    /// Publishes retained availability state.
    #[instrument(skip(self))]
    pub async fn publish_availability(&self, available: bool) -> Result<(), Error> {
        let payload = if available { "online" } else { "offline" };
        self.publish(self.availability_topic(), true, payload).await
    }

    /// Gives the event loop time to transmit queued messages.
    #[instrument(skip(self))]
    pub async fn flush(&self) -> Result<(), Error> {
        self.client
            .disconnect()
            .await
            .map_err(|source| Error::Client { source })?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    /// Returns the shared JSON state topic.
    fn state_topic(&self) -> String {
        format!("{}/state", self.topic_prefix)
    }

    /// Returns the shared availability topic.
    fn availability_topic(&self) -> String {
        format!("{}/status", self.topic_prefix)
    }

    /// Publishes one MQTT payload.
    async fn publish<T>(&self, topic: String, retain: bool, payload: T) -> Result<(), Error>
    where
        T: Into<Vec<u8>>,
    {
        self.client
            .publish(topic, QoS::AtLeastOnce, retain, payload)
            .await
            .map_err(|source| Error::Client { source })
    }
}

impl Drop for MqttPublisher {
    fn drop(&mut self) {
        self.event_loop.abort();
    }
}

/// Describes MQTT connection settings.
pub struct MqttConfig {
    /// MQTT broker host name or address.
    pub host: String,
    /// MQTT broker TCP port.
    pub port: u16,
    /// MQTT client identifier.
    pub client_id: String,
    /// Optional broker credentials.
    pub credentials: Option<MqttCredentials>,
    /// Home Assistant discovery topic prefix.
    pub discovery_prefix: String,
    /// Topic prefix for hafox state topics.
    pub topic_prefix: String,
}

/// Stores MQTT credentials.
pub struct MqttCredentials {
    /// MQTT user name.
    pub username: String,
    /// MQTT password.
    pub password: String,
}

/// Reports MQTT publishing failures.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that the broker connection failed.
    #[error("MQTT connection failed")]
    Connection {
        /// MQTT connection error source.
        #[source]
        source: ConnectionError,
    },
    /// Indicates that a client request failed.
    #[error("MQTT client request failed")]
    Client {
        /// MQTT client error source.
        #[source]
        source: rumqttc::ClientError,
    },
    /// Indicates that a payload could not be encoded.
    #[error("MQTT payload could not be encoded")]
    Json {
        /// JSON encoding error source.
        #[source]
        source: serde_json::Error,
    },
}

/// Describes one Home Assistant sensor.
#[derive(Clone, Debug)]
struct SensorDefinition {
    /// Unique Home Assistant entity identifier.
    unique_id: String,
    /// Human-readable entity name.
    name: String,
    /// Template extracting the entity state from the shared payload.
    value_template: String,
    /// Home Assistant unit of measurement.
    unit: Option<&'static str>,
    /// Home Assistant device class.
    device_class: Option<&'static str>,
    /// Home Assistant state class.
    state_class: Option<&'static str>,
    /// Home Assistant icon.
    icon: Option<&'static str>,
}

impl SensorDefinition {
    /// Builds the Home Assistant discovery payload.
    fn discovery(&self, topic_prefix: &str, snapshot: &EnergySnapshot) -> DiscoveryPayload {
        DiscoveryPayload {
            name: self.name.clone(),
            object_id: self.unique_id.clone(),
            unique_id: self.unique_id.clone(),
            state_topic: format!("{topic_prefix}/state"),
            value_template: self.value_template.clone(),
            availability_topic: format!("{topic_prefix}/status"),
            payload_available: "online",
            payload_not_available: "offline",
            unit_of_measurement: self.unit,
            device_class: self.device_class,
            state_class: self.state_class,
            icon: self.icon,
            device: DevicePayload::from_snapshot(snapshot),
        }
    }
}

/// Describes a Home Assistant MQTT discovery payload.
#[derive(Debug, Serialize)]
struct DiscoveryPayload {
    /// Human-readable entity name.
    name: String,
    /// Preferred Home Assistant object identifier.
    object_id: String,
    /// Stable Home Assistant unique identifier.
    unique_id: String,
    /// MQTT topic carrying state JSON.
    state_topic: String,
    /// Home Assistant template extracting the state value.
    value_template: String,
    /// MQTT topic carrying availability state.
    availability_topic: String,
    /// Payload representing availability.
    payload_available: &'static str,
    /// Payload representing unavailability.
    payload_not_available: &'static str,
    /// Home Assistant unit of measurement.
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_of_measurement: Option<&'static str>,
    /// Home Assistant device class.
    #[serde(skip_serializing_if = "Option::is_none")]
    device_class: Option<&'static str>,
    /// Home Assistant state class.
    #[serde(skip_serializing_if = "Option::is_none")]
    state_class: Option<&'static str>,
    /// Home Assistant icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'static str>,
    /// Home Assistant device metadata.
    device: DevicePayload,
}

/// Describes the Home Assistant device attached to all entities.
#[derive(Debug, Serialize)]
struct DevicePayload {
    /// Stable device identifier.
    identifiers: [&'static str; 1],
    /// Human-readable device name.
    name: &'static str,
    /// Device manufacturer.
    manufacturer: &'static str,
    /// Firmware version reported by SmartFox.
    sw_version: String,
}

impl DevicePayload {
    /// Builds device metadata from a snapshot.
    fn from_snapshot(snapshot: &EnergySnapshot) -> Self {
        Self {
            identifiers: ["hafox_smartfox"],
            name: "SmartFox",
            manufacturer: "SmartFox",
            sw_version: snapshot.system.firmware_version.clone(),
        }
    }
}

/// Describes the shared MQTT state payload.
#[derive(Debug, Serialize)]
struct StatePayload {
    /// Unix timestamp when the payload was produced.
    timestamp: u64,
    /// Live power measurements.
    power: PowerPayload,
    /// Cumulative energy counters.
    energy: EnergyPayload,
    /// Optional battery measurements.
    #[serde(skip_serializing_if = "Option::is_none")]
    battery: Option<BatteryPayload>,
    /// Per-phase electrical measurements.
    phases: PhasePayload,
}

impl StatePayload {
    /// Builds a state payload from a normalized snapshot.
    fn from_snapshot(snapshot: &EnergySnapshot) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        Self {
            timestamp,
            power: PowerPayload {
                solar_production_w: snapshot.power.production.watts,
                grid_net_w: snapshot.power.grid_net.watts,
                battery_w: snapshot.power.battery_power.map(|power| -power.watts),
                site_consumption_w: snapshot.power.consumption.watts,
            },
            energy: EnergyPayload {
                grid_import_wh: snapshot.energy.grid_import.watt_hours,
                grid_export_wh: snapshot.energy.grid_export.watt_hours,
                solar_production_wh: snapshot.energy.solar_production.watt_hours,
            },
            battery: snapshot.battery.as_ref().map(|battery| BatteryPayload {
                state_of_charge_pct: battery.state_of_charge.percent,
                temperature_c: battery.temperature.map(|temperature| temperature.celsius),
            }),
            phases: PhasePayload::from_phases(&snapshot.phases),
        }
    }
}

/// Describes live power measurements.
#[derive(Debug, Serialize)]
struct PowerPayload {
    /// Solar production in watts.
    solar_production_w: i64,
    /// Signed net grid power in watts.
    grid_net_w: i64,
    /// Signed battery power in watts, with charging as negative.
    #[serde(skip_serializing_if = "Option::is_none")]
    battery_w: Option<i64>,
    /// Site consumption in watts.
    site_consumption_w: i64,
}

/// Describes cumulative energy counters.
#[derive(Debug, Serialize)]
struct EnergyPayload {
    /// Cumulative grid import in watt-hours.
    grid_import_wh: i64,
    /// Cumulative grid export in watt-hours.
    grid_export_wh: i64,
    /// Cumulative solar production in watt-hours.
    solar_production_wh: i64,
}

/// Describes battery measurements.
#[derive(Debug, Serialize)]
struct BatteryPayload {
    /// Battery state of charge in percent.
    state_of_charge_pct: f64,
    /// Battery temperature in degrees Celsius.
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature_c: Option<f64>,
}

/// Describes all available phase measurements.
#[derive(Debug, Default, Serialize)]
struct PhasePayload {
    /// First AC phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    l1: Option<SinglePhasePayload>,
    /// Second AC phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    l2: Option<SinglePhasePayload>,
    /// Third AC phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    l3: Option<SinglePhasePayload>,
}

impl PhasePayload {
    /// Builds phase payloads from normalized phase measurements.
    fn from_phases(phases: &[PhaseMeasurement]) -> Self {
        let mut payload = Self::default();
        for phase in phases {
            let value = SinglePhasePayload {
                voltage_v: phase.voltage.volts,
                current_a: phase.current.amperes,
                power_w: phase.power.watts,
            };
            match phase.phase {
                GridPhase::L1 => payload.l1 = Some(value),
                GridPhase::L2 => payload.l2 = Some(value),
                GridPhase::L3 => payload.l3 = Some(value),
            }
        }

        payload
    }
}

/// Describes one phase measurement payload.
#[derive(Debug, Serialize)]
struct SinglePhasePayload {
    /// Phase voltage in volts.
    voltage_v: f64,
    /// Phase current in amperes.
    current_a: f64,
    /// Signed phase power in watts.
    power_w: i64,
}

/// Returns Home Assistant sensors for the available snapshot fields.
fn sensor_definitions(snapshot: &EnergySnapshot) -> Vec<SensorDefinition> {
    let mut sensors = vec![
        power_sensor(
            "solar_production_power",
            "Solar production power",
            "{{ value_json.power.solar_production_w }}",
        ),
        power_sensor(
            "grid_net_power",
            "Grid net power",
            "{{ value_json.power.grid_net_w }}",
        ),
        power_sensor(
            "site_consumption_power",
            "Site consumption power",
            "{{ value_json.power.site_consumption_w }}",
        ),
        energy_sensor(
            "grid_import_energy_total",
            "Grid import energy total",
            "{{ value_json.energy.grid_import_wh }}",
        ),
        energy_sensor(
            "grid_export_energy_total",
            "Grid export energy total",
            "{{ value_json.energy.grid_export_wh }}",
        ),
        energy_sensor(
            "solar_production_energy_total",
            "Solar production energy total",
            "{{ value_json.energy.solar_production_wh }}",
        ),
    ];

    if snapshot.power.battery_power.is_some() {
        sensors.push(power_sensor(
            "battery_power",
            "Battery power",
            "{{ value_json.power.battery_w }}",
        ));
    }

    if let Some(battery) = &snapshot.battery {
        sensors.push(SensorDefinition {
            unique_id: unique_id("battery_state_of_charge"),
            name: "Battery state of charge".to_owned(),
            value_template: "{{ value_json.battery.state_of_charge_pct }}".to_owned(),
            unit: Some("%"),
            device_class: Some("battery"),
            state_class: Some("measurement"),
            icon: None,
        });

        if battery.temperature.is_some() {
            sensors.push(SensorDefinition {
                unique_id: unique_id("battery_temperature"),
                name: "Battery temperature".to_owned(),
                value_template: "{{ value_json.battery.temperature_c }}".to_owned(),
                unit: Some("°C"),
                device_class: Some("temperature"),
                state_class: Some("measurement"),
                icon: None,
            });
        }
    }

    for phase in &snapshot.phases {
        let phase_name = match phase.phase {
            GridPhase::L1 => "l1",
            GridPhase::L2 => "l2",
            GridPhase::L3 => "l3",
        };
        let label = phase_name.to_uppercase();
        sensors.push(power_sensor(
            &format!("{phase_name}_power"),
            &format!("{label} power"),
            &format!("{{{{ value_json.phases.{phase_name}.power_w }}}}"),
        ));
        sensors.push(SensorDefinition {
            unique_id: unique_id(&format!("{phase_name}_voltage")),
            name: format!("{label} voltage"),
            value_template: format!("{{{{ value_json.phases.{phase_name}.voltage_v }}}}"),
            unit: Some("V"),
            device_class: Some("voltage"),
            state_class: Some("measurement"),
            icon: None,
        });
        sensors.push(SensorDefinition {
            unique_id: unique_id(&format!("{phase_name}_current")),
            name: format!("{label} current"),
            value_template: format!("{{{{ value_json.phases.{phase_name}.current_a }}}}"),
            unit: Some("A"),
            device_class: Some("current"),
            state_class: Some("measurement"),
            icon: None,
        });
    }

    sensors
}

/// Builds a power sensor definition.
fn power_sensor(suffix: &str, name: &str, value_template: &str) -> SensorDefinition {
    SensorDefinition {
        unique_id: unique_id(suffix),
        name: name.to_owned(),
        value_template: value_template.to_owned(),
        unit: Some("W"),
        device_class: Some("power"),
        state_class: Some("measurement"),
        icon: None,
    }
}

/// Builds an energy sensor definition.
fn energy_sensor(suffix: &str, name: &str, value_template: &str) -> SensorDefinition {
    SensorDefinition {
        unique_id: unique_id(suffix),
        name: name.to_owned(),
        value_template: value_template.to_owned(),
        unit: Some("Wh"),
        device_class: Some("energy"),
        state_class: Some("total_increasing"),
        icon: None,
    }
}

/// Builds a stable Home Assistant unique identifier.
fn unique_id(suffix: &str) -> String {
    format!("hafox_smartfox_{suffix}")
}

#[cfg(test)]
mod tests {
    use super::sensor_definitions;
    use crate::model::{
        BatteryState, Current, Energy, EnergySnapshot, EnergyTotals, GridPhase, Percent,
        PhaseMeasurement, Power, PowerFlow, SystemStatus, Temperature, Voltage,
    };

    /// Builds sensors for available optional fields.
    #[test]
    fn builds_sensor_definitions() {
        let snapshot = EnergySnapshot {
            system: SystemStatus {
                date: "2026-06-25".parse().expect("date should parse"),
                time: "01:07:09".parse().expect("time should parse"),
                ip_address: "10.97.59.174".parse().expect("IP should parse"),
                firmware_version: "EM3".to_owned(),
            },
            power: PowerFlow {
                production: Power { watts: 1 },
                grid_net: Power { watts: 2 },
                battery_power: Some(Power { watts: -3 }),
                consumption: Power { watts: 4 },
            },
            energy: EnergyTotals {
                grid_import: Energy { watt_hours: 5 },
                grid_export: Energy { watt_hours: 6 },
                solar_production: Energy { watt_hours: 7 },
            },
            battery: Some(BatteryState {
                state_of_charge: Percent { percent: 8.0 },
                temperature: Some(Temperature { celsius: 9.0 }),
            }),
            inverters: Vec::new(),
            phases: vec![PhaseMeasurement {
                phase: GridPhase::L1,
                voltage: Voltage { volts: 10.0 },
                current: Current { amperes: 11.0 },
                power: Power { watts: 12 },
            }],
        };

        let sensors = sensor_definitions(&snapshot);

        assert_eq!(sensors.len(), 12);
        assert!(
            sensors
                .iter()
                .any(|sensor| sensor.unique_id == "hafox_smartfox_battery_power")
        );
    }
}
