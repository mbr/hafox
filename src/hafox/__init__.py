#!/usr/bin/env python3
"""SmartFox energy monitor CLI tool."""

import json
import logging
import signal
import sys
import time
import xml.etree.ElementTree as ET
from datetime import datetime
from typing import Dict, Optional

import click
import requests
from rich.console import Console
from rich.table import Table
from rich.live import Live
from rich.panel import Panel
from rich.layout import Layout
from tabulate import tabulate

try:
    from .mqtt import SmartFoxMQTTPublisher
except ImportError:
    from hafox.mqtt import SmartFoxMQTTPublisher


console = Console()


def fetch_smartfox_data(
    base_url: str = "http://smartfox", timeout: int = 5
) -> Optional[str]:
    """Fetch XML data from SmartFox device."""
    url = f"{base_url}/values.xml"
    headers = {
        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:145.0) Gecko/20100101 Firefox/145.0",
        "Accept": "*/*",
        "Referer": f"{base_url}/",
    }

    try:
        response = requests.get(url, headers=headers, timeout=timeout)
        response.raise_for_status()
        return response.text
    except requests.RequestException as e:
        console.print(f"[red]Error fetching data: {e}[/red]")
        return None


def send_reboot_request(base_url: str, timeout: int = 5) -> bool:
    """Send reboot request to SmartFox device."""
    reboot_url = f"{base_url}/devrest.cgi"

    headers = {
        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:146.0) Gecko/20100101 Firefox/146.0",
        "Accept": "*/*",
        "Accept-Language": "en-US,en;q=0.5",
        "Accept-Encoding": "gzip, deflate",
        "Connection": "keep-alive",
        "Referer": f"{base_url}/einstellungen.shtml",
        "Priority": "u=0",
    }

    try:
        response = requests.get(reboot_url, headers=headers, timeout=timeout)
        response.raise_for_status()
        return True
    except requests.RequestException as e:
        logging.error(f"Failed to send reboot request: {e}")
        return False


def parse_xml_data(xml_data: str) -> Dict[str, str]:
    """Parse XML data and return a dictionary of values."""
    values = {}
    try:
        root = ET.fromstring(xml_data)
        for value_elem in root.findall(".//value"):
            id_attr = value_elem.get("id")
            if id_attr and value_elem.text:
                text = value_elem.text.strip()
                text = text.replace("&lt;span&gt;", " ").replace("&lt;/span&gt;", "")
                text = text.replace("&#176;", "°").replace("&#x25;", "%")
                values[id_attr] = text
    except ET.ParseError as e:
        console.print(f"[red]Error parsing XML: {e}[/red]")
    return values


def create_overview_table(values: Dict[str, str]) -> Table:
    """Create a rich table with overview data."""
    table = Table(
        title="SmartFox Energy Monitor", show_header=True, header_style="bold magenta"
    )
    table.add_column("Category", style="cyan", width=20)
    table.add_column("Metric", style="green")
    table.add_column("Value", style="yellow", justify="right")

    # System Info
    table.add_row(
        "System",
        "Date/Time",
        f"{values.get('dateValue', 'N/A')} {values.get('timeValue', 'N/A')}",
    )
    table.add_row("System", "IP Address", values.get("ipAddress", "N/A"))
    table.add_row("System", "Firmware", values.get("version", "N/A"))

    # Grid Status
    table.add_row("Grid", "Power", values.get("hidPower", "N/A"))
    table.add_row("Grid", "To Grid", values.get("toGridValue", "N/A"))
    table.add_row("Grid", "Current Power", values.get("detailsPowerValue", "N/A"))

    # Energy
    table.add_row("Energy", "Total", values.get("energyValue", "N/A"))
    table.add_row("Energy", "Today", values.get("eDayValue", "N/A"))
    table.add_row("Energy", "To Grid Total", values.get("eToGridValue", "N/A"))
    table.add_row("Energy", "To Grid Today", values.get("eDayToGridValue", "N/A"))

    # Solar
    table.add_row("Solar", "Production", values.get("hidProduction", "N/A"))
    table.add_row("Solar", "Inverter Power", values.get("wr1PowerValue", "N/A"))
    table.add_row("Solar", "Inverter Energy", values.get("wr1EnergyValue", "N/A"))

    # Battery (if available)
    if "batterySoc" in values and values["batterySoc"] != "-1%":
        table.add_row("Battery", "SOC", values.get("batterySoc", "N/A"))
        table.add_row("Battery", "Power", values.get("battery1Power", "N/A"))
        table.add_row(
            "Battery", "Temperature", values.get("battery1Temperature", "N/A")
        )

    return table


def create_phases_table(values: Dict[str, str]) -> Table:
    """Create a table with phase details."""
    table = Table(title="Phase Details", show_header=True, header_style="bold blue")
    table.add_column("Phase", style="cyan")
    table.add_column("Voltage", style="green", justify="right")
    table.add_column("Current", style="yellow", justify="right")
    table.add_column("Power", style="magenta", justify="right")

    for phase in ["L1", "L2", "L3"]:
        voltage = values.get(f"voltage{phase}Value", "N/A")
        current = values.get(f"ampere{phase}Value", "N/A")
        power = values.get(f"power{phase}Value", "N/A")
        table.add_row(phase, voltage, current, power)

    return table


def display_simple(values: Dict[str, str]) -> None:
    """Display data in simple text format."""
    print(f"\n{'=' * 70}")
    print(f"SmartFox Energy Monitor - {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"{'=' * 70}\n")

    sections = {
        "SYSTEM INFO": [
            "dateValue:Date",
            "timeValue:Time",
            "ipAddress:IP Address",
            "version:Firmware",
        ],
        "GRID STATUS": [
            "hidPower:Grid Power",
            "toGridValue:To Grid",
            "detailsPowerValue:Current Power",
        ],
        "ENERGY": [
            "energyValue:Total Energy",
            "eDayValue:Energy Today",
            "eToGridValue:Energy To Grid Total",
            "eDayToGridValue:Energy To Grid Today",
        ],
        "SOLAR PRODUCTION": [
            "hidProduction:Production",
            "wr1PowerValue:Inverter 1 Power",
            "wr1EnergyValue:Inverter 1 Energy",
        ],
    }

    for section, fields in sections.items():
        print(f"{section}")
        print("-" * 35)
        for field in fields:
            key, label = field.split(":")
            if key in values:
                print(f"{label:.<25} {values[key]:>25}")
        print()


@click.group()
@click.version_option(version="0.1.0")
def cli():
    """SmartFox energy monitor CLI tool."""
    pass


@cli.command()
@click.option(
    "-u",
    "--url",
    default="http://smartfox",
    help="SmartFox base URL",
    show_default=True,
)
@click.option(
    "-f",
    "--format",
    "output_format",
    type=click.Choice(["rich", "simple", "json"]),
    default="rich",
    help="Output format",
    show_default=True,
)
@click.option(
    "-w", "--watch", is_flag=True, help="Watch mode - refresh every 5 seconds"
)
@click.option(
    "-i",
    "--interval",
    default=5,
    help="Refresh interval in seconds (for watch mode)",
    show_default=True,
)
def monitor(url: str, output_format: str, watch: bool, interval: int):
    """Display current SmartFox energy monitor data."""

    def display_once():
        xml_data = fetch_smartfox_data(url)
        if not xml_data:
            return False

        values = parse_xml_data(xml_data)

        if output_format == "json":
            console.print_json(json.dumps(values, indent=2))
        elif output_format == "simple":
            display_simple(values)
        elif output_format == "rich":
            console.print(create_overview_table(values))
            console.print()
            console.print(create_phases_table(values))

        return True

    if watch:
        console.print(
            f"[green]Watching SmartFox data (refresh every {interval}s, press Ctrl+C to stop)...[/green]"
        )
        try:
            while True:
                console.clear()
                display_once()
                time.sleep(interval)
        except KeyboardInterrupt:
            console.print("\n[yellow]Stopped watching.[/yellow]")
    else:
        display_once()


@cli.command()
@click.option(
    "-u",
    "--url",
    default="http://smartfox",
    help="SmartFox base URL",
    show_default=True,
)
@click.option("-k", "--key", help="Specific value key to retrieve")
def get(url: str, key: Optional[str]):
    """Get specific value(s) from SmartFox."""
    xml_data = fetch_smartfox_data(url)
    if not xml_data:
        sys.exit(1)

    values = parse_xml_data(xml_data)

    if key:
        if key in values:
            console.print(values[key])
        else:
            console.print(f"[red]Key '{key}' not found[/red]")
            sys.exit(1)
    else:
        # List all available keys
        console.print("[cyan]Available keys:[/cyan]")
        for k in sorted(values.keys()):
            console.print(f"  {k}: {values[k]}")


@cli.command()
@click.option(
    "-u",
    "--url",
    default="http://smartfox",
    help="SmartFox base URL",
    show_default=True,
)
def export(url: str):
    """Export all SmartFox data as JSON."""
    xml_data = fetch_smartfox_data(url)
    if not xml_data:
        sys.exit(1)

    values = parse_xml_data(xml_data)
    values["timestamp"] = datetime.now().isoformat()

    print(json.dumps(values, indent=2))


@cli.command()
@click.option("-h", "--host", required=True, help="MQTT broker host")
@click.option("-p", "--port", default=1883, help="MQTT broker port", show_default=True)
@click.option("-u", "--username", help="MQTT username")
@click.option("-P", "--password", help="MQTT password")
@click.option(
    "-i",
    "--interval",
    default=30,
    help="Polling interval in seconds",
    show_default=True,
)
@click.option(
    "--discovery/--no-discovery",
    default=True,
    help="Enable Home Assistant discovery",
    show_default=True,
)
@click.option(
    "--topic-prefix", default="smartfox", help="MQTT topic prefix", show_default=True
)
@click.option(
    "--device-id", default="smartfox", help="Device identifier", show_default=True
)
@click.option(
    "--smartfox-url",
    default="http://smartfox",
    help="SmartFox base URL",
    show_default=True,
)
@click.option(
    "--reboot-interval",
    type=int,
    help="Send device reboot request every N seconds (WARNING: reboots the device)",
)
@click.option("-v", "--verbose", is_flag=True, help="Enable verbose logging")
def publish(
    host: str,
    port: int,
    username: Optional[str],
    password: Optional[str],
    interval: int,
    discovery: bool,
    topic_prefix: str,
    device_id: str,
    smartfox_url: str,
    reboot_interval: Optional[int],
    verbose: bool,
):
    """Publish SmartFox data to MQTT broker for Home Assistant integration."""

    # Setup logging
    log_level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=log_level, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    )
    log = logging.getLogger(__name__)

    console.print(f"[green]Starting SmartFox MQTT publisher[/green]")
    console.print(f"MQTT: {host}:{port}")
    console.print(f"SmartFox: {smartfox_url}")
    console.print(f"Interval: {interval}s")
    console.print(f"Discovery: {'enabled' if discovery else 'disabled'}")
    if reboot_interval:
        console.print(
            f"[yellow]Reboot interval: {reboot_interval}s (WARNING: Device will reboot periodically)[/yellow]"
        )

    # Create MQTT publisher
    mqtt_publisher = SmartFoxMQTTPublisher(
        host=host,
        port=port,
        username=username,
        password=password,
        topic_prefix=topic_prefix,
        device_id=device_id,
        discovery=discovery,
    )

    # Global flag for clean shutdown
    running = True
    last_reboot_time = None

    def signal_handler(signum, frame):
        nonlocal running
        console.print(f"\n[yellow]Received signal {signum}, shutting down...[/yellow]")
        running = False

    # Register signal handlers
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    # Initialize last reboot time if reboot interval is configured
    if reboot_interval:
        log.info(f"Reboot interval configured: {reboot_interval} seconds")

    # Connect to MQTT broker
    if not mqtt_publisher.connect():
        console.print("[red]Failed to connect to MQTT broker[/red]")
        sys.exit(1)

    console.print("[green]Connected to MQTT broker[/green]")

    # Main loop
    consecutive_smartfox_failures = 0
    consecutive_mqtt_failures = 0

    while running:
        try:
            # Check if it's time to send reboot request
            if reboot_interval:
                current_time = time.time()
                if (
                    last_reboot_time is None
                    or (current_time - last_reboot_time) >= reboot_interval
                ):
                    log.warning("Sending reboot request to SmartFox device")
                    success = send_reboot_request(smartfox_url)
                    if success:
                        log.info("Reboot request sent successfully")
                    else:
                        log.error("Failed to send reboot request")
                    last_reboot_time = current_time

            # Fetch SmartFox data
            log.info("Fetching SmartFox data")
            xml_data = fetch_smartfox_data(smartfox_url)

            if xml_data is None:
                consecutive_smartfox_failures += 1
                log.error(
                    f"SmartFox fetch failed ({consecutive_smartfox_failures} consecutive failures)"
                )
                if consecutive_smartfox_failures <= 3:
                    time.sleep(5)  # Quick retry
                    continue
                else:
                    # Slower retry for persistent failures
                    time.sleep(min(30, 5 * consecutive_smartfox_failures))
                    continue
            else:
                if consecutive_smartfox_failures > 0:
                    log.info("SmartFox connection restored")
                    consecutive_smartfox_failures = 0

            # Parse data
            values = parse_xml_data(xml_data)
            if not values:
                log.error("No data parsed from SmartFox")
                time.sleep(5)
                continue

            # Publish to MQTT
            if mqtt_publisher.connected:
                success = mqtt_publisher.publish_sensors(values)
                if success:
                    if consecutive_mqtt_failures > 0:
                        log.info("MQTT publishing restored")
                        consecutive_mqtt_failures = 0
                else:
                    consecutive_mqtt_failures += 1
                    log.error(
                        f"MQTT publish failed ({consecutive_mqtt_failures} consecutive failures)"
                    )
            else:
                consecutive_mqtt_failures += 1
                log.error("MQTT not connected, attempting reconnection")
                if not mqtt_publisher.reconnect():
                    time.sleep(5)
                    continue

            # Wait for next cycle
            time.sleep(interval)

        except KeyboardInterrupt:
            break
        except Exception as e:
            log.error(f"Unexpected error in main loop: {e}")
            time.sleep(10)  # Longer wait for unexpected errors

    # Cleanup
    console.print("[yellow]Shutting down...[/yellow]")
    mqtt_publisher.disconnect()
    console.print("[green]Shutdown complete[/green]")


def main():
    """Main entry point."""
    cli()


if __name__ == "__main__":
    main()
