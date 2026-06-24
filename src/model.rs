//! Domain model for SmartFox measurements.

use std::{num::ParseFloatError, str::FromStr};

use serde::Serialize;
use thiserror::Error;

use crate::smartfox::SmartFoxValues;

/// Stores a power value in kilowatts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Power {
    /// Power normalized to kilowatts.
    pub kilowatts: f64,
}

impl FromStr for Power {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (amount, unit) = parse_amount_and_unit(value)?;
        let kilowatts = match unit.as_deref() {
            Some("kW") => amount,
            Some("W") => amount / 1000.0,
            Some(unit) => {
                return Err(MeasurementParseError::UnsupportedUnit {
                    expected: "W or kW",
                    unit: unit.to_owned(),
                });
            }
            None => {
                return Err(MeasurementParseError::MissingUnit {
                    expected: "W or kW",
                });
            }
        };

        Ok(Self { kilowatts })
    }
}

/// Stores an energy value in kilowatt-hours.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Energy {
    /// Energy normalized to kilowatt-hours.
    pub kilowatt_hours: f64,
}

impl FromStr for Energy {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (amount, unit) = parse_amount_and_unit(value)?;
        let kilowatt_hours = match unit.as_deref() {
            Some("kWh") => amount,
            Some("Wh") => amount / 1000.0,
            Some(unit) => {
                return Err(MeasurementParseError::UnsupportedUnit {
                    expected: "Wh or kWh",
                    unit: unit.to_owned(),
                });
            }
            None => {
                return Err(MeasurementParseError::MissingUnit {
                    expected: "Wh or kWh",
                });
            }
        };

        Ok(Self { kilowatt_hours })
    }
}

/// Stores a percentage value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Percent {
    /// Percentage as a value in the range reported by the device.
    pub percent: f64,
}

impl FromStr for Percent {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = normalize_value(value);
        let amount = normalized.trim().trim_end_matches('%').trim();
        if amount.is_empty() {
            return Err(MeasurementParseError::Empty);
        }

        let percent = amount
            .parse()
            .map_err(|source| MeasurementParseError::InvalidNumber { source })?;
        Ok(Self { percent })
    }
}

/// Stores a temperature value in Celsius.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Temperature {
    /// Temperature normalized to degrees Celsius.
    pub celsius: f64,
}

impl FromStr for Temperature {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (amount, unit) = parse_amount_and_unit(value)?;
        match unit.as_deref() {
            Some("°C") => Ok(Self { celsius: amount }),
            Some(unit) => Err(MeasurementParseError::UnsupportedUnit {
                expected: "°C",
                unit: unit.to_owned(),
            }),
            None => Err(MeasurementParseError::MissingUnit { expected: "°C" }),
        }
    }
}

/// Stores an electrical potential in volts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Voltage {
    /// Electrical potential normalized to volts.
    pub volts: f64,
}

impl FromStr for Voltage {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (amount, unit) = parse_amount_and_unit(value)?;
        match unit.as_deref() {
            Some("V") => Ok(Self { volts: amount }),
            Some(unit) => Err(MeasurementParseError::UnsupportedUnit {
                expected: "V",
                unit: unit.to_owned(),
            }),
            None => Err(MeasurementParseError::MissingUnit { expected: "V" }),
        }
    }
}

/// Stores an electrical current in amperes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Current {
    /// Electrical current normalized to amperes.
    pub amperes: f64,
}

impl FromStr for Current {
    type Err = MeasurementParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (amount, unit) = parse_amount_and_unit(value)?;
        match unit.as_deref() {
            Some("A") => Ok(Self { amperes: amount }),
            Some(unit) => Err(MeasurementParseError::UnsupportedUnit {
                expected: "A",
                unit: unit.to_owned(),
            }),
            None => Err(MeasurementParseError::MissingUnit { expected: "A" }),
        }
    }
}

/// Describes one snapshot of normalized SmartFox data.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EnergySnapshot {
    /// Device identity and clock information.
    pub system: SystemStatus,
    /// Live power flow across the installation boundary.
    pub power: PowerFlow,
    /// Cumulative energy counters suitable for long-lived consumers.
    pub energy: EnergyTotals,
    /// Battery state selected with the same rules as the SmartFox live view.
    pub battery: Option<BatteryState>,
    /// Solar inverter measurements reported by the device.
    pub inverters: Vec<Inverter>,
    /// Per-phase electrical measurements reported by the device.
    pub phases: Vec<PhaseMeasurement>,
}

impl EnergySnapshot {
    /// Builds a normalized snapshot from raw SmartFox values.
    pub fn from_smartfox_values(values: &SmartFoxValues) -> Result<Self, Error> {
        let production: Power = required_measurement(values, "hidProduction")?;
        let grid: Power = required_measurement(values, "hidPower")?;
        let battery_power_key = selected_battery_key(values, "Power");
        let battery_power = optional_measurement(values, &battery_power_key)?;
        let battery_soc_key = selected_battery_key(values, "Soc");
        let battery_soc = optional_measurement(values, &battery_soc_key)?;
        let battery_temperature = optional_measurement(values, "battery1Temperature")?;
        let inverters = inverter_measurements(values)?;
        let phases = phase_measurements(values)?;
        let battery_power_kw = battery_power
            .map(|power: Power| power.kilowatts)
            .unwrap_or(0.0);
        let consumption = Power {
            kilowatts: (production.kilowatts + grid.kilowatts - battery_power_kw).max(0.0),
        };
        let power = PowerFlow {
            production,
            grid,
            grid_import: Power {
                kilowatts: grid.kilowatts.max(0.0),
            },
            grid_export: Power {
                kilowatts: (-grid.kilowatts).max(0.0),
            },
            battery: battery_power,
            consumption,
        };
        let energy = EnergyTotals {
            grid_import: required_measurement(values, "energyValue")?,
            grid_export: required_measurement(values, "eToGridValue")?,
            solar_production: Energy {
                kilowatt_hours: inverters
                    .iter()
                    .map(|inverter| inverter.total_energy.kilowatt_hours)
                    .sum(),
            },
        };
        let battery = battery_soc.map(|state_of_charge| BatteryState {
            state_of_charge,
            power: battery_power,
            temperature: battery_temperature,
        });

        Ok(Self {
            system: SystemStatus {
                date: values.get("dateValue").map(ToOwned::to_owned),
                time: values.get("timeValue").map(ToOwned::to_owned),
                ip_address: values.get("ipAddress").map(ToOwned::to_owned),
                firmware_version: values.get("version").map(ToOwned::to_owned),
            },
            power,
            energy,
            battery,
            inverters,
            phases,
        })
    }
}

/// Describes device identity and clock information.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SystemStatus {
    /// Calendar date reported by the device.
    pub date: Option<String>,
    /// Clock time reported by the device.
    pub time: Option<String>,
    /// Network address reported by the device.
    pub ip_address: Option<String>,
    /// Firmware version reported by the device.
    pub firmware_version: Option<String>,
}

/// Describes live power flow values.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PowerFlow {
    /// Solar production measured by the SmartFox controller.
    pub production: Power,
    /// Signed grid power measured at the grid boundary.
    pub grid: Power,
    /// Non-negative grid import derived from [`PowerFlow::grid`].
    pub grid_import: Power,
    /// Non-negative grid export derived from [`PowerFlow::grid`].
    pub grid_export: Power,
    /// Signed battery power selected from live-view battery fields.
    pub battery: Option<Power>,
    /// Site consumption derived from production, grid, and battery power.
    pub consumption: Power,
}

/// Describes cumulative energy counters.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EnergyTotals {
    /// Cumulative energy imported from the grid.
    pub grid_import: Energy,
    /// Cumulative energy exported to the grid.
    pub grid_export: Energy,
    /// Cumulative solar energy summed from inverter counters.
    pub solar_production: Energy,
}

/// Describes the selected battery state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatteryState {
    /// Battery state of charge.
    pub state_of_charge: Percent,
    /// Signed battery power, where negative values represent discharge.
    pub power: Option<Power>,
    /// Battery temperature when available.
    pub temperature: Option<Temperature>,
}

/// Describes one solar inverter.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Inverter {
    /// One-based inverter index from the SmartFox payload.
    pub index: u8,
    /// Live inverter output power.
    pub power: Power,
    /// Cumulative inverter energy counter.
    pub total_energy: Energy,
}

/// Names one AC phase.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum GridPhase {
    /// First AC phase.
    L1,
    /// Second AC phase.
    L2,
    /// Third AC phase.
    L3,
}

/// Describes one phase measurement.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhaseMeasurement {
    /// Phase identity.
    pub phase: GridPhase,
    /// Phase voltage.
    pub voltage: Voltage,
    /// Phase current.
    pub current: Current,
    /// Signed phase power.
    pub power: Power,
}

/// Reports measurement parsing failures.
#[derive(Debug, Error)]
pub enum MeasurementParseError {
    /// Indicates that no measurement text was present.
    #[error("measurement is empty")]
    Empty,
    /// Indicates that the numeric part could not be parsed.
    #[error("measurement number is invalid")]
    InvalidNumber {
        /// Underlying float parser error.
        #[source]
        source: ParseFloatError,
    },
    /// Indicates that a required unit was absent.
    #[error("measurement unit is missing, expected {expected}")]
    MissingUnit {
        /// Expected unit description.
        expected: &'static str,
    },
    /// Indicates that the unit cannot be normalized.
    #[error("measurement unit `{unit}` is unsupported, expected {expected}")]
    UnsupportedUnit {
        /// Unit found in the payload.
        unit: String,
        /// Expected unit description.
        expected: &'static str,
    },
}

/// Reports model construction failures.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that a required SmartFox field was missing.
    #[error("SmartFox field `{field}` is missing")]
    MissingField {
        /// Missing SmartFox field name.
        field: String,
    },
    /// Indicates that a SmartFox field could not be converted.
    #[error("SmartFox field `{field}` has invalid value `{value}`")]
    InvalidMeasurement {
        /// SmartFox field name.
        field: String,
        /// Raw SmartFox field value.
        value: String,
        /// Measurement parsing failure.
        #[source]
        source: MeasurementParseError,
    },
}

/// Returns a normalized measurement string.
fn normalize_value(value: &str) -> String {
    value
        .replace(',', ".")
        .replace("&lt;span&gt;", " ")
        .replace("&lt;/span&gt;", "")
        .replace("<span>", " ")
        .replace("</span>", "")
        .replace("Â°C", "°C")
}

/// Splits a measurement into a number and unit.
fn parse_amount_and_unit(value: &str) -> Result<(f64, Option<String>), MeasurementParseError> {
    let normalized = normalize_value(value);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return Err(MeasurementParseError::Empty);
    }

    let unit_start = normalized
        .find(|character: char| {
            !character.is_ascii_digit() && character != '-' && character != '+' && character != '.'
        })
        .unwrap_or(normalized.len());
    let amount = normalized[..unit_start]
        .parse()
        .map_err(|source| MeasurementParseError::InvalidNumber { source })?;
    let unit = normalized[unit_start..].trim();
    let unit = (!unit.is_empty()).then(|| unit.to_owned());

    Ok((amount, unit))
}

/// Reads and parses a required measurement field.
fn required_measurement<T>(values: &SmartFoxValues, field: &str) -> Result<T, Error>
where
    T: FromStr<Err = MeasurementParseError>,
{
    let value = values.get(field).ok_or_else(|| Error::MissingField {
        field: field.to_owned(),
    })?;
    parse_field(field, value)
}

/// Reads and parses an optional measurement field.
fn optional_measurement<T>(values: &SmartFoxValues, field: &str) -> Result<Option<T>, Error>
where
    T: FromStr<Err = MeasurementParseError>,
{
    values
        .get(field)
        .map(|value| parse_field(field, value))
        .transpose()
}

/// Parses one field into the requested measurement type.
fn parse_field<T>(field: &str, value: &str) -> Result<T, Error>
where
    T: FromStr<Err = MeasurementParseError>,
{
    value.parse().map_err(|source| Error::InvalidMeasurement {
        field: field.to_owned(),
        value: value.to_owned(),
        source,
    })
}

/// Selects the battery key used by the SmartFox live view.
fn selected_battery_key(values: &SmartFoxValues, suffix: &str) -> String {
    let is_luna = (1..=3).any(|index| values.get(&format!("hidBsHuawei2Luna{index}")) == Some("1"));
    if is_luna && values.get("hidBsProd") == Some("18") {
        return format!("battery1{suffix}");
    }

    let preferred = format!("battery1{suffix}1");
    if values.get(&preferred).is_some() {
        preferred
    } else {
        format!("battery1{suffix}")
    }
}

/// Builds inverter measurements from indexed SmartFox fields.
fn inverter_measurements(values: &SmartFoxValues) -> Result<Vec<Inverter>, Error> {
    let mut inverters = Vec::new();
    for index in 1..=5 {
        let power_key = format!("wr{index}PowerValue");
        let energy_key = format!("wr{index}EnergyValue");
        let power = optional_measurement(values, &power_key)?;
        let total_energy = optional_measurement(values, &energy_key)?;
        if let (Some(power), Some(total_energy)) = (power, total_energy) {
            inverters.push(Inverter {
                index,
                power,
                total_energy,
            });
        }
    }

    Ok(inverters)
}

/// Builds phase measurements from SmartFox phase fields.
fn phase_measurements(values: &SmartFoxValues) -> Result<Vec<PhaseMeasurement>, Error> {
    let phase_fields = [
        (
            GridPhase::L1,
            "voltageL1Value",
            "ampereL1Value",
            "powerL1Value",
        ),
        (
            GridPhase::L2,
            "voltageL2Value",
            "ampereL2Value",
            "powerL2Value",
        ),
        (
            GridPhase::L3,
            "voltageL3Value",
            "ampereL3Value",
            "powerL3Value",
        ),
    ];
    let mut phases = Vec::new();

    for (phase, voltage_key, current_key, power_key) in phase_fields {
        let voltage = optional_measurement(values, voltage_key)?;
        let current = optional_measurement(values, current_key)?;
        let power = optional_measurement(values, power_key)?;
        if let (Some(voltage), Some(current), Some(power)) = (voltage, current, power) {
            phases.push(PhaseMeasurement {
                phase,
                voltage,
                current,
                power,
            });
        }
    }

    Ok(phases)
}

#[cfg(test)]
mod tests {
    use super::{Energy, EnergySnapshot, Power, Temperature};
    use crate::smartfox::SmartFoxValues;

    /// Parses common measurement units into normalized values.
    #[test]
    fn parses_measurements() {
        assert_eq!(
            "1500 W".parse::<Power>().expect("power should parse"),
            Power { kilowatts: 1.5 }
        );
        assert_eq!(
            "25 Wh".parse::<Energy>().expect("energy should parse"),
            Energy {
                kilowatt_hours: 0.025
            }
        );
        assert_eq!(
            "31°C"
                .parse::<Temperature>()
                .expect("temperature should parse"),
            Temperature { celsius: 31.0 }
        );
    }

    /// Derives live power flow and cumulative solar energy.
    #[test]
    fn builds_snapshot() {
        let values = SmartFoxValues::from_pairs([
            ("hidProduction", "2.00 kW"),
            ("hidPower", "-500 W"),
            ("battery1Power1", "-1.00 kW"),
            ("battery1Soc1", "80%"),
            ("battery1Temperature", "30 °C"),
            ("energyValue", "100.000 kWh"),
            ("eToGridValue", "40.000 kWh"),
            ("wr1PowerValue", "2.00 kW"),
            ("wr1EnergyValue", "500.00 kWh"),
        ]);

        let snapshot =
            EnergySnapshot::from_smartfox_values(&values).expect("snapshot should build");

        assert_eq!(snapshot.power.grid_import.kilowatts, 0.0);
        assert_eq!(snapshot.power.grid_export.kilowatts, 0.5);
        assert_eq!(snapshot.power.consumption.kilowatts, 2.5);
        assert_eq!(snapshot.energy.solar_production.kilowatt_hours, 500.0);
    }
}
