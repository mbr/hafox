#!/usr/bin/env python3
"""Fetch and display SmartFox values."""

import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime


def fetch_smartfox_data():
    """Fetch XML data from SmartFox device."""
    url = 'http://smartfox/values.xml'
    
    headers = {
        'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:145.0) Gecko/20100101 Firefox/145.0',
        'Accept': '*/*',
        'Referer': 'http://smartfox/'
    }
    
    req = urllib.request.Request(url, headers=headers)
    
    try:
        with urllib.request.urlopen(req) as response:
            return response.read().decode('utf-8')
    except Exception as e:
        print(f"Error fetching data: {e}")
        return None


def parse_and_display(xml_data):
    """Parse XML and display values in a readable format."""
    try:
        root = ET.fromstring(xml_data)
        
        print(f"\n{'='*70}")
        print(f"SmartFox Energy Monitor - {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
        print(f"{'='*70}\n")
        
        # Key values to display
        key_values = {
            'toGridValue': 'To Grid',
            'detailsPowerValue': 'Current Power',
            'energyValue': 'Total Energy',
            'eDayValue': 'Energy Today',
            'eToGridValue': 'Energy To Grid Total',
            'eDayToGridValue': 'Energy To Grid Today',
            'voltageL1Value': 'Voltage L1',
            'voltageL2Value': 'Voltage L2', 
            'voltageL3Value': 'Voltage L3',
            'ampereL1Value': 'Current L1',
            'ampereL2Value': 'Current L2',
            'ampereL3Value': 'Current L3',
            'powerL1Value': 'Power L1',
            'powerL2Value': 'Power L2',
            'powerL3Value': 'Power L3',
            'wr1PowerValue': 'Inverter 1 Power',
            'wr1EnergyValue': 'Inverter 1 Energy',
            'batterySoc': 'Battery SOC',
            'battery1Power': 'Battery Power',
            'battery1Temperature': 'Battery Temperature',
            'hidProduction': 'Production',
            'hidPower': 'Grid Power',
            'dateValue': 'Date',
            'timeValue': 'Time',
            'ipAddress': 'IP Address',
            'macAddress': 'MAC Address',
            'version': 'Firmware'
        }
        
        # Extract and display key values
        values = {}
        for value_elem in root.findall('.//value'):
            id_attr = value_elem.get('id')
            if id_attr and value_elem.text:
                # Clean HTML entities from the text
                text = value_elem.text.strip()
                text = text.replace('&lt;span&gt;', ' ').replace('&lt;/span&gt;', '')
                text = text.replace('&#176;', '°').replace('&#x25;', '%')
                values[id_attr] = text
        
        # System Info
        print("SYSTEM INFO")
        print("-" * 35)
        for key in ['dateValue', 'timeValue', 'ipAddress', 'macAddress', 'version']:
            if key in values:
                label = key_values.get(key, key)
                print(f"{label:.<25} {values[key]:>25}")
        
        # Grid Status
        print("\nGRID STATUS")
        print("-" * 35)
        for key in ['hidPower', 'toGridValue', 'detailsPowerValue']:
            if key in values:
                label = key_values.get(key, key)
                print(f"{label:.<25} {values[key]:>25}")
        
        # Energy
        print("\nENERGY")
        print("-" * 35)
        for key in ['energyValue', 'eDayValue', 'eToGridValue', 'eDayToGridValue']:
            if key in values:
                label = key_values.get(key, key)
                print(f"{label:.<25} {values[key]:>25}")
        
        # Phase Details
        print("\nPHASE DETAILS")
        print("-" * 35)
        for phase in ['L1', 'L2', 'L3']:
            voltage_key = f'voltage{phase}Value'
            current_key = f'ampere{phase}Value'
            power_key = f'power{phase}Value'
            
            if voltage_key in values:
                print(f"{phase} - Voltage:.<18} {values[voltage_key]:>25}")
            if current_key in values:
                print(f"{phase} - Current:.<18} {values[current_key]:>25}")
            if power_key in values:
                print(f"{phase} - Power:.<20} {values[power_key]:>25}")
        
        # Solar Production
        print("\nSOLAR PRODUCTION")
        print("-" * 35)
        for key in ['hidProduction', 'wr1PowerValue', 'wr1EnergyValue']:
            if key in values:
                label = key_values.get(key, key)
                print(f"{label:.<25} {values[key]:>25}")
        
        # Battery Status (if available)
        if 'batterySoc' in values and values['batterySoc'] != '-1%':
            print("\nBATTERY STATUS")
            print("-" * 35)
            for key in ['batterySoc', 'battery1Power', 'battery1Temperature']:
                if key in values:
                    label = key_values.get(key, key)
                    print(f"{label:.<25} {values[key]:>25}")
        
        print(f"\n{'='*70}\n")
        
    except ET.ParseError as e:
        print(f"Error parsing XML: {e}")
        print("\nRaw XML data:")
        print(xml_data)


def main():
    """Main function."""
    print("Fetching SmartFox data...")
    xml_data = fetch_smartfox_data()
    
    if xml_data:
        parse_and_display(xml_data)
    else:
        print("Failed to fetch data from SmartFox")


if __name__ == "__main__":
    main()