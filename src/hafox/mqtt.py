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

        # Sensor definitions with Home Assistant device classes
        self.sensors = self._define_sensors()

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
            # Power sensors (real-time)
            "grid_power": {
                "name": "Grid Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidPower",
                "icon": "mdi:transmission-tower",
            },
            "grid_export": {
                "name": "Grid Export",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "toGridValue",
                "convert": lambda x: float(x.split()[0]) * 1000,  # kW to W
                "icon": "mdi:transmission-tower-export",
            },
            "solar_production": {
                "name": "Solar Production",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "hidProduction",
                "convert": lambda x: float(x.split()[0]) * 1000,  # kW to W
                "icon": "mdi:solar-power",
            },
            "battery_power": {
                "name": "Battery Power",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "battery1Power",
                "convert": lambda x: float(x.split()[0]) * 1000,  # kW to W
                "icon": "mdi:battery-charging",
            },
            # Energy sensors (cumulative)
            "energy_total": {
                "name": "Total Energy Consumed",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "energyValue",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:lightning-bolt",
            },
            "energy_today": {
                "name": "Energy Consumed Today",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "eDayValue",
                "convert": lambda x: float(x.split()[0]) / 1000,  # Wh to kWh
                "icon": "mdi:lightning-bolt",
            },
            "energy_export_total": {
                "name": "Total Energy Exported",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "eToGridValue",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:transmission-tower-export",
            },
            "energy_export_today": {
                "name": "Energy Exported Today",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "eDayToGridValue",
                "convert": lambda x: float(x.split()[0]) / 1000,  # Wh to kWh
                "icon": "mdi:transmission-tower-export",
            },
            "solar_energy_total": {
                "name": "Total Solar Energy",
                "device_class": "energy",
                "unit_of_measurement": "kWh",
                "state_class": "total_increasing",
                "smartfox_key": "wr1EnergyValue",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:solar-power",
            },
            # Battery sensors
            "battery_soc": {
                "name": "Battery State of Charge",
                "device_class": "battery",
                "unit_of_measurement": "%",
                "state_class": "measurement",
                "smartfox_key": "batterySoc",
                "convert": lambda x: int(x.replace("%", "")),
                "icon": "mdi:battery",
            },
            "battery_temperature": {
                "name": "Battery Temperature",
                "device_class": "temperature",
                "unit_of_measurement": "°C",
                "state_class": "measurement",
                "smartfox_key": "battery1Temperature",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:thermometer",
            },
            # Phase sensors
            "voltage_l1": {
                "name": "Voltage L1",
                "device_class": "voltage",
                "unit_of_measurement": "V",
                "state_class": "measurement",
                "smartfox_key": "voltageL1Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:sine-wave",
            },
            "voltage_l2": {
                "name": "Voltage L2",
                "device_class": "voltage",
                "unit_of_measurement": "V",
                "state_class": "measurement",
                "smartfox_key": "voltageL2Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:sine-wave",
            },
            "voltage_l3": {
                "name": "Voltage L3",
                "device_class": "voltage",
                "unit_of_measurement": "V",
                "state_class": "measurement",
                "smartfox_key": "voltageL3Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:sine-wave",
            },
            "current_l1": {
                "name": "Current L1",
                "device_class": "current",
                "unit_of_measurement": "A",
                "state_class": "measurement",
                "smartfox_key": "ampereL1Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:current-ac",
            },
            "current_l2": {
                "name": "Current L2",
                "device_class": "current",
                "unit_of_measurement": "A",
                "state_class": "measurement",
                "smartfox_key": "ampereL2Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:current-ac",
            },
            "current_l3": {
                "name": "Current L3",
                "device_class": "current",
                "unit_of_measurement": "A",
                "state_class": "measurement",
                "smartfox_key": "ampereL3Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:current-ac",
            },
            "power_l1": {
                "name": "Power L1",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "powerL1Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:lightning-bolt",
            },
            "power_l2": {
                "name": "Power L2",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "powerL2Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:lightning-bolt",
            },
            "power_l3": {
                "name": "Power L3",
                "device_class": "power",
                "unit_of_measurement": "W",
                "state_class": "measurement",
                "smartfox_key": "powerL3Value",
                "convert": lambda x: float(x.split()[0]),
                "icon": "mdi:lightning-bolt",
            },
        }

        # Add device info to all sensors
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
        pass  # Silent success

    def publish_discovery(self):
        """Publish Home Assistant MQTT discovery messages."""
        if not self.discovery:
            return

        log.info("Publishing Home Assistant discovery messages")

        for sensor_id, config in self.sensors.items():
            discovery_topic = (
                f"homeassistant/sensor/{self.device_id}_{sensor_id}/config"
            )

            # Build discovery payload
            discovery_payload = {
                "name": config["name"],
                "unique_id": config["unique_id"],
                "object_id": config["object_id"],
                "state_topic": config["state_topic"],
                "device": config["device"],
            }

            # Add optional fields
            for field in ["device_class", "unit_of_measurement", "state_class", "icon"]:
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
                # Apply conversion if specified
                if "convert" in config:
                    value = config["convert"](raw_value)
                else:
                    value = raw_value

                # Publish to MQTT
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
