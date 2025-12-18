# hafox

SmartFox energy monitor CLI tool with Home Assistant MQTT integration. Reads `values.xml` from the plain HTTP endpoint offered by smartfox and publishes read values to MQTT.

Integrates with home-assistant autodiscovery.

## Usage

This software is packaged as a nix flake (and only that), it is not published in PyPI at the moment. Contact me if you need that to change.

## Usage

Display current stats with `hafox monitor` or `hafax monitor -w` to refresh. For other options, see `hafox --help`.

## MQTT Publishing for Home Assistant

Running `hafox publish -h your.mqtt.broker` will publish to said broker, see `hafox publish --help` for additional options, such as credentials.

You can test if it is working through `mosquitto_sub -h your.mqtt.broker -u USERNAME -P PASSWORD -t 'smartfox/#' -v -C 10`
