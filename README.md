# hafox

`hafox` is a small utility that reads the local [SmartFox](https://smartfox.at/) `values.xml` endpoint and publishes it to MQTT, in a format that [Home Assistant](https://www.home-assistant.io/) understands.

## Quickstart

Run the app by either compiling it from source using `cargo build --release` or through `nix run git+ssh://git@github.com/mbr/hafox.git`, e.g.

```
nix run git+ssh://git@github.com/mbr/hafox.git -- dump
```

The simplemost command is `dump`, which in the default configuration connects to a SmartFox device running under http://smartfox (the default). You should see a data dump:

```
EnergySnapshot {
    system: SystemStatus {
        date: 2026-06-25,
        time: 23:35:03,
        ip_address: 10.97.59.174,
        firmware_version: "EM3  00.01.10.02",
    },
    power: PowerFlow {
        production: Power {
            watts: 0,
        },
        grid_net: Power {
            watts: 19,
        },
        battery_power: Some(
            Power {
                watts: -1140,
            },
        ),
        consumption: Power {
            watts: 1159,
        },
    },
    energy: EnergyTotals {
        grid_import: Energy {
        watt_hours: 2991999,
    },
    grid_export: Energy {
        watt_hours: 5339753,
    },
    solar_production: Energy {
        watt_hours: 13499800,
    },
...    
```

If these values seem plausible, retrieval worked correctly.

**Note**: `hafox` has not seen a multitude of configuration yet, e.g. if you do not have a battery attached, it should work, but it has never been tested against such a setup.

## Exporting to MQTT/Home Assistant

`hafox export` publishes information retrieved from SmartFox via MQTT; this process is split, the *discovery* information tells Home Assistant which sensors exist:

```
homeassistant/sensor/hafox_smartfox_solar_production_power/config
homeassistant/sensor/hafox_smartfox_grid_net_power/config
...


Once discovery information is written, the *state* is updated once:

```
hafox/state
hafox/status
```

Both of these values are retained. The `hafox/state` payload is JSON:

```json
{
  "timestamp": 1782424028,
  "power": {
    "solar_production_w": 0,
    "grid_net_w": 51,
    "battery_w": 1100,
    "site_consumption_w": 1151
  },
  "energy": {
    "grid_import_wh": 2992004,
    "grid_export_wh": 5339753,
    "solar_production_wh": 13499800
  },
  "battery": {
    "state_of_charge_pct": 49.0,
    "temperature_c": 31.0
  },
  "phases": {
    "l1": {
      "voltage_v": 235.0,
      "current_a": 1.66,
      "power_w": 180
    },
    "l2": {
      "voltage_v": 236.0,
      "current_a": 3.16,
      "power_w": 187
    },
    "l3": {
      "voltage_v": 234.0,
      "current_a": 1.39,
      "power_w": -316
    }
  }
}
```

The state topic is also retained, so intermittent outages of `hafox` have no catastrophic effects like resetting the meter in Home Assistant. Run `hafox export --help` for a list of configuration options to configure MQTT server credentials.

## Continuous export

For continuous updates, use `run`:

```sh
hafox run --mqtt-host myserver --refresh-interval 5s ...
```

This will publish discovery information to MQTT once at the start, writing only state updates every `refresh-interval` seconds afterwards.

Logging can we configured using `RUST_LOG`, the default of `hafox=INFO` will print one log message per update.

## NixOS

The flake exposes `nixosModules.default`:

```nix
{
  imports = [ inputs.hafox.nixosModules.default ];

  services.hafox = {
    enable = true;
    smartfoxUrl = "http://smartfox";
    refreshInterval = "5s";

    mqtt = {
      host = "myserver";
      username = "hafox";
      passwordFile = "/run/secrets/hafox-mqtt-password";
    };
  };
}
```

Use `mqtt.passwordFile` for production secrets. `mqtt.password` is available for non-secret deployments but stores the value in the Nix configuration.
