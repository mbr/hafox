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


def parse_number(value: Optional[str]) -> Optional[float]:
    """Parse the numeric prefix of a SmartFox value."""
    if not value:
        return None

    parts = value.replace(",", ".").split()
    if not parts:
        return None

    try:
        return float(parts[0])
    except ValueError:
        return None


def parse_power_kw(value: Optional[str]) -> Optional[float]:
    """Parse a SmartFox power value as kilowatts."""
    power = parse_number(value)
    if power is None:
        return None

    parts = value.split() if value else []
    unit = parts[1] if len(parts) > 1 else "kW"
    if unit == "W":
        return power / 1000
    if unit == "kW":
        return power
    return None


def parse_energy_kwh(
    value: Optional[str], default_unit: str = "kWh"
) -> Optional[float]:
    """Parse a SmartFox energy value as kilowatt-hours."""
    energy = parse_number(value)
    if energy is None:
        return None

    parts = value.split() if value else []
    unit = parts[1] if len(parts) > 1 else default_unit
    if unit == "Wh":
        return energy / 1000
    if unit == "kWh":
        return energy
    return None


def format_power_kw(value: float) -> str:
    """Format a power value in kilowatts."""
    return f"{value:.2f} kW"


def format_energy_kwh(value: float) -> str:
    """Format an energy value in kilowatt-hours."""
    return f"{value:.3f} kWh"


def selected_battery_key(values: Dict[str, str], suffix: str) -> str:
    """Select the battery value key used by the SmartFox live view."""
    is_luna = any(values.get(f"hidBsHuawei2Luna{i}") == "1" for i in range(1, 4))
    if is_luna and values.get("hidBsProd") == "18":
        return f"battery1{suffix}"

    preferred = f"battery1{suffix}1"
    if preferred in values:
        return preferred
    return f"battery1{suffix}"


def calculate_consumption_power_kw(values: Dict[str, str]) -> Optional[float]:
    """Calculate current consumption using SmartFox live view semantics."""
    production = parse_power_kw(values.get("hidProduction"))
    grid = parse_power_kw(values.get("hidPower"))
    battery = parse_power_kw(values.get(selected_battery_key(values, "Power"))) or 0

    if production is None or grid is None:
        return None

    return max(0, production + grid - battery)


def calculate_solar_energy_total_kwh(values: Dict[str, str]) -> float:
    """Calculate cumulative solar energy from all inverter counters."""
    total = 0.0
    for index in range(1, 6):
        total += parse_energy_kwh(values.get(f"wr{index}EnergyValue")) or 0
    return total


def add_derived_values(values: Dict[str, str]) -> Dict[str, str]:
    """Add computed values that are not provided directly by SmartFox."""
    battery_power = values.get(selected_battery_key(values, "Power"))
    if battery_power is not None:
        values["batteryPower"] = battery_power

    battery_soc = values.get(selected_battery_key(values, "Soc"))
    if battery_soc is not None:
        values["batterySocLive"] = battery_soc

    consumption = calculate_consumption_power_kw(values)
    if consumption is not None:
        values["consumptionPower"] = format_power_kw(consumption)

    values["solarEnergyTotal"] = format_energy_kwh(
        calculate_solar_energy_total_kwh(values)
    )

    grid = parse_power_kw(values.get("hidPower"))
    if grid is not None:
        values["gridImportPower"] = format_power_kw(max(grid, 0))
        values["gridExportPower"] = format_power_kw(max(-grid, 0))

    return values


def grid_power_status(values: Dict[str, str]) -> tuple[str, str]:
    """Return the live grid direction and absolute power."""
    grid = parse_power_kw(values.get("hidPower"))
    if grid is None:
        return "Grid Power", values.get("hidPower", "N/A")
    if grid >= 0:
        return "From Grid", format_power_kw(grid)
    return "To Grid", format_power_kw(-grid)


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
                text = text.replace("<span>", " ").replace("</span>", "")
                text = text.replace("&#176;", "°").replace("&#x25;", "%")
                text = text.replace("Â°C", "°C")
                values[id_attr] = text
    except ET.ParseError as e:
        console.print(f"[red]Error parsing XML: {e}[/red]")
    return add_derived_values(values)


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

    # Power flow
    grid_label, grid_value = grid_power_status(values)
    table.add_row("Power", "Consumption", values.get("consumptionPower", "N/A"))
    table.add_row("Power", "Production", values.get("hidProduction", "N/A"))
    table.add_row("Power", grid_label, grid_value)

    # Energy
    table.add_row("Energy", "Grid Import Total", values.get("energyValue", "N/A"))
    table.add_row("Energy", "Grid Export Total", values.get("eToGridValue", "N/A"))
    table.add_row("Energy", "Inverter 1 Total", values.get("wr1EnergyValue", "N/A"))

    # Solar
    table.add_row("Solar", "Production", values.get("hidProduction", "N/A"))
    table.add_row("Solar", "Inverter Power", values.get("wr1PowerValue", "N/A"))
    table.add_row("Solar", "Inverter Energy", values.get("wr1EnergyValue", "N/A"))

    # Battery (if available)
    if "batterySoc" in values and values["batterySoc"] != "-1%":
        table.add_row("Battery", "SOC", values.get("batterySocLive", "N/A"))
        table.add_row("Battery", "Power", values.get("batteryPower", "N/A"))
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

    grid_label, grid_value = grid_power_status(values)
    sections = {
        "SYSTEM INFO": [
            ("dateValue", "Date"),
            ("timeValue", "Time"),
            ("ipAddress", "IP Address"),
            ("version", "Firmware"),
        ],
        "POWER": [
            ("consumptionPower", "Consumption"),
            ("hidProduction", "Production"),
            (None, grid_label, grid_value),
            ("batteryPower", "Battery Power"),
        ],
        "ENERGY": [
            ("energyValue", "Grid Import Total"),
            ("eToGridValue", "Grid Export Total"),
            ("wr1EnergyValue", "Inverter 1 Total"),
        ],
        "SOLAR PRODUCTION": [
            ("wr1PowerValue", "Inverter 1 Power"),
            ("wr1EnergyValue", "Inverter 1 Energy"),
        ],
    }

    for section, fields in sections.items():
        print(f"{section}")
        print("-" * 35)
        for field in fields:
            if len(field) == 3:
                _, label, value = field
                print(f"{label:.<25} {value:>25}")
                continue

            key, label = field
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

    console.print("[green]Starting SmartFox MQTT publisher[/green]")
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
