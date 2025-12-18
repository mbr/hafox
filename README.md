# hafox

SmartFox energy monitor CLI tool with Home Assistant MQTT integration.

## Features

- **Monitor** SmartFox energy data with real-time display
- **Export** data as JSON
- **Publish** to MQTT for Home Assistant integration
- **Auto-discovery** for 20+ sensors (power, energy, battery, phases)
- **Robust error handling** with aggressive reconnection

## Installation

```bash
nix develop  # Enter development shell
```

## Usage

### Monitor SmartFox Data

```bash
# One-time display
hafox monitor

# Live monitoring (refresh every 5s)
hafox monitor -w -i 5

# JSON output
hafox monitor -f json

# Simple text format
hafox monitor -f simple
```

### Get Specific Values

```bash
# List all available keys
hafox get

# Get specific value
hafox get -k batterySoc
hafox get -k hidProduction
```

### Export Data

```bash
# Export all data as JSON
hafox export

# Save to file
hafox export > data.json
```

### MQTT Publishing for Home Assistant

```bash
# Basic usage
hafox publish -h your.mqtt.broker

# Full configuration
hafox publish \
  -h mqtt.local \
  -u username \
  -P password \
  -i 30 \
  --topic-prefix smartfox \
  --device-id smartfox \
  -v
```

**Published Sensors:**
- **Power**: Grid power, solar production, battery power, grid export
- **Energy**: Total consumption, daily totals, solar generation
- **Battery**: State of charge, temperature
- **Phases**: L1/L2/L3 voltage, current, power
- **System**: Device status

## Systemd Service

Create `/etc/systemd/system/hafox.service`:

```ini
[Unit]
Description=SmartFox MQTT Publisher
After=network.target

[Service]
Type=simple
User=hafox
ExecStart=/path/to/hafox publish -h mqtt.local -u user -P pass
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable hafox
sudo systemctl start hafox
```

## Development

```bash
# Format code
./format.sh

# Test MQTT locally
mosquitto -p 1883 &
mosquitto_sub -h localhost -t 'smartfox/#' -v &
hafox publish -h localhost -v
```

## Options

### Global Options
- `-u, --url` - SmartFox URL (default: http://smartfox/values.xml)
- `-v, --verbose` - Enable verbose logging

### Monitor Options
- `-f, --format` - Output format: rich, simple, json (default: rich)
- `-w, --watch` - Live monitoring mode
- `-i, --interval` - Refresh interval in seconds (default: 30)

### MQTT Options
- `-h, --host` - MQTT broker host (required)
- `-p, --port` - MQTT broker port (default: 1883)
- `-u, --username` - MQTT username
- `-P, --password` - MQTT password
- `--discovery/--no-discovery` - Enable Home Assistant discovery (default: enabled)
- `--topic-prefix` - MQTT topic prefix (default: smartfox)
- `--device-id` - Device identifier (default: smartfox)

## Error Handling

- **SmartFox failures**: Exponential backoff with aggressive retry
- **MQTT failures**: Immediate reconnection attempts  
- **Signal handling**: Clean shutdown on SIGTERM/SIGINT
- **Logging**: Full error visibility for systemd journal