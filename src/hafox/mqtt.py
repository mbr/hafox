#!/usr/bin/env python3
"""MQTT publisher for SmartFox data with Home Assistant discovery."""

import json
import logging
import time
from typing import Dict, Optional

import paho.mqtt.client as mqtt
from rich.console import Console

console = Console()
log = logging.getLogger(__name__)


def numeric_prefix(value: str) -> float:
    """Parse the numeric prefix of a SmartFox value."""
    normalized = (
        value.replace(",", ".")
        .replace("<span>", " ")
        .replace("</span>", "")
        .replace("Â°C", "°C")
    )
    return float(normalized.split()[0])


def power_w(value: str) -> float:
    """Convert a SmartFox power value to watts."""
    power = numeric_prefix(value)
    normalized = value.replace("<span>", " ").replace("</span>", "")
    parts = normalized.split()
    unit = parts[1] if len(parts) > 1 else "W"
    if unit == "kW":
        return power * 1000
    return power


def energy_kwh(value: str) -> float:
    """Convert a SmartFox energy value to kilowatt-hours."""
    energy = numeric_prefix(value)
    parts = value.split()
    unit = parts[1] if len(parts) > 1 else "kWh"
    if unit == "Wh":
        return energy / 1000
    return energy


def percent(value: str) -> float:
    """Convert a SmartFox percentage value to a number."""
    return numeric_prefix(value.replace("%", ""))


def minutes(value: str) -> float:
    """Convert a SmartFox duration value to minutes."""
    return numeric_prefix(value)


class SmartFoxMQTTPublisher:
    """Publishes SmartFox data to MQTT with Home Assistant discovery."""

    def __init__(
        self,
        host: str,
        port: int = 1883,
        username: Optional[str] = None,
        password: Optional[str] = None,
        topic_prefix: str = "smartfox",
        device_id: str = "smartfox",
        discovery: bool = True,
    ):
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.topic_prefix = topic_prefix
        self.device_id = device_id
        self.discovery = discovery

        self.client = mqtt.Client()
        self.client.on_connect = self._on_connect
        self.client.on_disconnect = self._on_disconnect
        self.client.on_publish = self._on_publish

        if username and password:
            self.client.username_pw_set(username, password)

        self.connected = False
        self.discovery_published = False
        self.sensors = self._define_sensors()
        self.deprecated_entities = [
            ("sensor", "energy_today"),
            ("sensor", "energy_export_today"),
        ]

    def _define_sensors(self) -> Dict[str, Dict]:
        """Define all sensors with their Home Assistant configurations."""
        device_info = {
            "identifiers": [self.device_id],
            "name": "SmartFox Energy Monitor",
            "model": "EM3",
            "manufacturer": "SmartFox",
            "sw_version": "1.0",
        }

        sensors = {
            "consumption_power": {
                "name": "Power Consumption",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "consumptionPower",
                "convert": power_w,
                "icon": "mdi:home-lightning-bolt",
            },
            "grid_power": {
                "name": "Grid Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidPower",
                "convert": power_w,
                "icon": "mdi:transmission-tower",
            },
            "grid_import": {
                "name": "Grid Import",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "gridImportPower",
                "convert": power_w,
                "icon": "mdi:transmission-tower-import",
            },
            "grid_export": {
                "name": "Grid Export",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "gridExportPower",
                "convert": power_w,
                "icon": "mdi:transmission-tower-export",
            },
            "solar_production": {
                "name": "Solar Production",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidProduction",
                "convert": power_w,
                "icon": "mdi:solar-power",
            },
            "battery_power": {
                "name": "Battery Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "batteryPower",
                "convert": power_w,
                "icon": "mdi:battery-charging",
            },
            "energy_total": {
                "name": "Grid Import Energy Total",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "energyValue",
                "convert": energy_kwh,
                "icon": "mdi:transmission-tower-import",
            },
            "energy_export_total": {
                "name": "Grid Export Energy Total",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "eToGridValue",
                "convert": energy_kwh,
                "icon": "mdi:transmission-tower-export",
            },
            "solar_energy_total": {
                "name": "Solar Energy Total",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "solarEnergyTotal",
                "convert": energy_kwh,
                "icon": "mdi:solar-power",
            },
            "battery_soc": {
                "name": "Battery State of Charge",
                "device_class": "battery",
                "unit_of_measurement": "%",
                "state_class": "measurement",
                "smartfox_key": "batterySocLive",
                "convert": percent,
                "icon": "mdi:battery",
            },
            "battery_temperature": {
                "name": "Battery Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "battery1Temperature",
                "convert": numeric_prefix,
                "icon": "mdi:thermometer",
            },
            "analog_output_power": {
                "name": "Analog Output Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "analogOutPower",
                "convert": power_w,
                "icon": "mdi:current-dc",
            },
            "analog_output_percent": {
                "name": "Analog Output Percent",
                "unit_of_measurement": "%",
                "state_class": "measurement",
                "smartfox_key": "analogOutPercent",
                "convert": percent,
                "icon": "mdi:percent",
            },
            "analog_output_temperature": {
                "name": "Analog Output Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "analogOutTemp",
                "convert": numeric_prefix,
                "icon": "mdi:thermometer",
            },
            "heat_pump_power": {
                "name": "Heat Pump Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "wpPowerValue",
                "convert": power_w,
                "icon": "mdi:heat-pump",
            },
            "heat_pump_thermal_power": {
                "name": "Heat Pump Thermal Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "wpThermPowerValue",
                "convert": power_w,
                "icon": "mdi:heat-pump",
            },
            "heat_pump_state": {
                "name": "Heat Pump State",
                "smartfox_key": "wpStateValue",
                "icon": "mdi:heat-pump",
            },
            "heat_pump_buffer_temperature": {
                "name": "Heat Pump Buffer Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "wpTempBufferValue",
                "convert": numeric_prefix,
                "icon": "mdi:thermometer",
            },
            "heat_pump_water_temperature": {
                "name": "Heat Pump Water Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "wpTempWarmWaterValue",
                "convert": numeric_prefix,
                "icon": "mdi:thermometer-water",
            },
            "heat_pump_return_temperature": {
                "name": "Heat Pump Return Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "wpTempReturnValue",
                "convert": numeric_prefix,
                "icon": "mdi:thermometer",
            },
            "community_power": {
                "name": "Community Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidcommunity_power",
                "convert": power_w,
                "icon": "mdi:home-group",
            },
            "community_surplus": {
                "name": "Community Surplus",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidcommunity_my_surplus",
                "convert": power_w,
                "icon": "mdi:home-export-outline",
            },
            "version": {
                "name": "Version",
                "smartfox_key": "version",
                "icon": "mdi:information-outline",
                "entity_category": "diagnostic",
            },
            "ip_address": {
                "name": "IP Address",
                "smartfox_key": "ipAddress",
                "icon": "mdi:ip-network",
                "entity_category": "diagnostic",
            },
        }

        for phase in ["L1", "L2", "L3"]:
            phase_id = phase.lower()
            sensors[f"voltage_{phase_id}"] = {
                "name": f"Voltage {phase}",
                "device_class": "voltage",
                "unit_of_measurement": "V",
                "state_class": "measurement",
                "smartfox_key": f"voltage{phase}Value",
                "convert": numeric_prefix,
                "icon": "mdi:sine-wave",
            }
            sensors[f"current_{phase_id}"] = {
                "name": f"Current {phase}",
                "device_class": "current",
                "unit_of_measurement": "A",
                "state_class": "measurement",
                "smartfox_key": f"ampere{phase}Value",
                "convert": numeric_prefix,
                "icon": "mdi:current-ac",
            }
            sensors[f"power_{phase_id}"] = {
                "name": f"Power {phase}",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": f"power{phase}Value",
                "convert": power_w,
                "icon": "mdi:lightning-bolt",
            }

        for index in range(1, 6):
            sensors[f"inverter_{index}_power"] = {
                "name": f"Inverter {index} Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": f"wr{index}PowerValue",
                "convert": power_w,
                "icon": "mdi:solar-power",
            }
            sensors[f"inverter_{index}_energy_total"] = {
                "name": f"Inverter {index} Energy Total",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": f"wr{index}EnergyValue",
                "convert": energy_kwh,
                "icon": "mdi:solar-power",
            }
            sensors[f"car_charger_{index}_power"] = {
                "name": f"Car Charger {index} Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": f"cc{index}Power",
                "convert": power_w,
                "icon": "mdi:ev-station",
            }
            sensors[f"car_charger_{index}_last_charge"] = {
                "name": f"Car Charger {index} Last Charge",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "measurement",
                "smartfox_key": f"cc{index}LastChargeValue",
                "convert": energy_kwh,
                "icon": "mdi:ev-station",
            }
            sensors[f"external_meter_{index}_power"] = {
                "name": f"External Meter {index} Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": f"externalMeter{index}Power",
                "convert": power_w,
                "icon": "mdi:meter-electric",
            }

        for index in range(1, 5):
            sensors[f"relay_{index}_status"] = {
                "component": "binary_sensor",
                "name": f"Relay {index} Status",
                "smartfox_key": f"relayStatusValue{index}",
                "payload_on": "1",
                "payload_off": "0",
                "icon": "mdi:electric-switch",
            }
            sensors[f"relay_{index}_name"] = {
                "name": f"Relay {index} Name",
                "smartfox_key": f"hidR{index}Name",
                "icon": "mdi:label-outline",
                "entity_category": "diagnostic",
            }
            sensors[f"relay_{index}_remaining_time"] = {
                "name": f"Relay {index} Remaining Time",
                "unit_of_measurement": "min",
                "state_class": "measurement",
                "smartfox_key": f"relayRemTimeValue{index}",
                "convert": minutes,
                "icon": "mdi:timer-sand",
            }
            sensors[f"relay_{index}_runtime"] = {
                "name": f"Relay {index} Runtime",
                "unit_of_measurement": "min",
                "state_class": "measurement",
                "smartfox_key": f"relayRunTimeValue{index}",
                "convert": minutes,
                "icon": "mdi:timer-outline",
            }

        for index in range(1, 3):
            sensors[f"battery_{index}_soc"] = {
                "name": f"Battery {index} State of Charge",
                "device_class": "battery",
                "unit_of_measurement": "%",
                "state_class": "measurement",
                "smartfox_key": f"battery1Soc{index}",
                "convert": percent,
                "icon": "mdi:battery",
            }
            sensors[f"battery_{index}_power"] = {
                "name": f"Battery {index} Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": f"battery1Power{index}",
                "convert": power_w,
                "icon": "mdi:battery-charging",
            }

        for sensor_id, config in sensors.items():
            config["device"] = device_info
            config["unique_id"] = f"{self.device_id}_{sensor_id}"
            config["object_id"] = f"{self.device_id}_{sensor_id}"
            config["state_topic"] = f"{self.topic_prefix}/sensor/{sensor_id}/state"

        return sensors

    def connect(self) -> bool:
        """Connect to MQTT broker with retry logic."""
        try:
            log.info(f"Connecting to MQTT broker {self.host}:{self.port}")
            self.client.connect(self.host, self.port, 60)
            self.client.loop_start()
            return True
        except Exception as e:
            log.error(f"MQTT connection failed: {e}")
            return False

    def disconnect(self):
        """Disconnect from MQTT broker."""
        if self.connected:
            self.publish_device_status("offline")
            self.client.loop_stop()
            self.client.disconnect()

    def _on_connect(self, client, userdata, flags, rc):
        """Handle MQTT connection."""
        if rc == 0:
            log.info("Connected to MQTT broker")
            self.connected = True
            self.publish_device_status("online")
            if self.discovery and not self.discovery_published:
                self.publish_discovery()
        else:
            log.error(f"MQTT connection failed with code {rc}")
            self.connected = False

    def _on_disconnect(self, client, userdata, rc):
        """Handle MQTT disconnection."""
        log.warning(f"Disconnected from MQTT broker (rc={rc})")
        self.connected = False
        self.discovery_published = False

    def _on_publish(self, client, userdata, mid):
        """Handle successful publish."""
        pass

    def publish_discovery(self):
        """Publish Home Assistant discovery messages."""
        if not self.discovery:
            return

        log.info("Publishing Home Assistant discovery messages")

        for sensor_id, config in self.sensors.items():
            component = config.get("component", "sensor")
            discovery_topic = (
                f"homeassistant/{component}/{self.device_id}_{sensor_id}/config"
            )

            discovery_payload = {
                "name": config["name"],
                "unique_id": config["unique_id"],
                "object_id": config["object_id"],
                "state_topic": config["state_topic"],
                "device": config["device"],
            }

            fields = [
                "device_class",
                "unit_of_measurement",
                "state_class",
                "icon",
                "payload_on",
                "payload_off",
                "entity_category",
            ]
            for field in fields:
                if field in config:
                    discovery_payload[field] = config[field]

            try:
                result = self.client.publish(
                    discovery_topic, json.dumps(discovery_payload), retain=True
                )
                if result.rc != mqtt.MQTT_ERR_SUCCESS:
                    log.error(f"Failed to publish discovery for {sensor_id}")
                else:
                    log.debug(f"Published discovery for {sensor_id}")
            except Exception as e:
                log.error(f"Error publishing discovery for {sensor_id}: {e}")

        for component, sensor_id in self.deprecated_entities:
            discovery_topic = (
                f"homeassistant/{component}/{self.device_id}_{sensor_id}/config"
            )
            try:
                self.client.publish(discovery_topic, "", retain=True)
                log.debug(f"Cleared discovery for {sensor_id}")
            except Exception as e:
                log.error(f"Error clearing discovery for {sensor_id}: {e}")

        self.discovery_published = True

    def publish_device_status(self, status: str):
        """Publish device online/offline status."""
        topic = f"{self.topic_prefix}/status"
        try:
            self.client.publish(topic, status, retain=True)
            log.debug(f"Published device status: {status}")
        except Exception as e:
            log.error(f"Failed to publish device status: {e}")

    def publish_sensors(self, smartfox_data: Dict[str, str]):
        """Publish sensor data to MQTT."""
        if not self.connected:
            log.error("Not connected to MQTT broker")
            return False

        published_count = 0

        for sensor_id, config in self.sensors.items():
            smartfox_key = config["smartfox_key"]

            if smartfox_key not in smartfox_data:
                log.debug(f"SmartFox key {smartfox_key} not found for {sensor_id}")
                continue

            raw_value = smartfox_data[smartfox_key]

            try:
                if "convert" in config:
                    value = config["convert"](raw_value)
                else:
                    value = raw_value

                topic = config["state_topic"]
                result = self.client.publish(topic, str(value))

                if result.rc == mqtt.MQTT_ERR_SUCCESS:
                    published_count += 1
                    log.debug(f"Published {sensor_id}: {value}")
                else:
                    log.error(f"Failed to publish {sensor_id}")

            except Exception as e:
                log.error(f"Error processing {sensor_id} (value: {raw_value}): {e}")

        log.info(f"Published {published_count}/{len(self.sensors)} sensors")
        return published_count > 0

    def reconnect(self):
        """Attempt to reconnect to MQTT broker."""
        log.info("Attempting MQTT reconnection...")
        try:
            if self.connected:
                self.client.disconnect()
            time.sleep(1)
            return self.connect()
        except Exception as e:
            log.error(f"Reconnection failed: {e}")
            return False
