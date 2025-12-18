#!/usr/bin/env python3
"""SmartFox energy monitor CLI tool."""

import json
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


console = Console()


def fetch_smartfox_data(
    url: str = "http://smartfox/values.xml", timeout: int = 5
) -> Optional[str]:
    """Fetch XML data from SmartFox device."""
    headers = {
        "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:145.0) Gecko/20100101 Firefox/145.0",
        "Accept": "*/*",
        "Referer": "http://smartfox/",
    }

    try:
        response = requests.get(url, headers=headers, timeout=timeout)
        response.raise_for_status()
        return response.text
    except requests.RequestException as e:
        console.print(f"[red]Error fetching data: {e}[/red]")
        return None


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
@click.option("--url", default="http://smartfox/values.xml", help="SmartFox URL")
@click.option(
    "--format",
    "output_format",
    type=click.Choice(["rich", "simple", "json"]),
    default="rich",
    help="Output format",
)
@click.option("--watch", is_flag=True, help="Watch mode - refresh every 5 seconds")
@click.option(
    "--interval", default=5, help="Refresh interval in seconds (for watch mode)"
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
@click.option("--url", default="http://smartfox/values.xml", help="SmartFox URL")
@click.option("--key", help="Specific value key to retrieve")
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
@click.option("--url", default="http://smartfox/values.xml", help="SmartFox URL")
def export(url: str):
    """Export all SmartFox data as JSON."""
    xml_data = fetch_smartfox_data(url)
    if not xml_data:
        sys.exit(1)

    values = parse_xml_data(xml_data)
    values["timestamp"] = datetime.now().isoformat()

    print(json.dumps(values, indent=2))


def main():
    """Main entry point."""
    cli()


if __name__ == "__main__":
    main()
