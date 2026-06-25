# hafox

`hafox` is a small utility that reads the local [SmartFox](https://smartfox.at/) `values.xml` endpoint and publishes it to MQTT, in a format that [Home Assistant](https://www.home-assistant.io/) understands.

## Quickstart

Run the app by either compiling it from source with `cargo build --release` or with `nix run git+ssh://git@github.com/mbr/hafox.git`, e.g.

```
nix run git+ssh://git@github.com/mbr/hafox.git -- dump
```

The simplest command is `dump`, which in the default configuration connects to a SmartFox device at `http://smartfox`. You should see a data dump:

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

**Note**: `hafox` has not been tested with many configurations yet. For example, it should work without an attached battery, but that setup has not been tested.

## Exporting to MQTT/Home Assistant

`hafox export` publishes information retrieved from SmartFox via MQTT; this process is split: the *discovery* information tells Home Assistant which sensors exist:

```
homeassistant/sensor/hafox_smartfox_solar_production_power/config
homeassistant/sensor/hafox_smartfox_grid_net_power/config
...
```

Once discovery information is written, the *state* is updated once:

```
hafox/state
hafox/status
```

Both of these topics are retained. The `hafox/state` payload is JSON:

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

The state topic is retained, so intermittent outages of `hafox` do not reset meters in Home Assistant. Run `hafox export --help` for MQTT broker configuration options.

## Continuous export

For continuous updates, use `run`:

```sh
hafox run --mqtt-host myserver --refresh-interval 5s ...
```

This publishes discovery information to MQTT once at startup, then writes only state updates every `--refresh-interval` seconds afterwards.

Logging can be configured using `RUST_LOG`. The default, `hafox=info`, prints one log message per update.

## Home Assistant setup

After MQTT discovery entities have been written, open *Settings*, *Dashboards*, *Energy*.

For *Electricity grid*, add grid consumption with:

```
sensor.hafox_smartfox_grid_import_energy_total
```

and return to grid with:

```
sensor.hafox_smartfox_grid_export_energy_total
```

For *Solar panels*, add solar production with:

```
sensor.hafox_smartfox_solar_production_energy_total
```

*Home battery storage* is not supported, since SmartFox only exposes battery power, state of charge, and temperature, while Home Assistant [requires lifetime energy measurement](https://www.home-assistant.io/docs/energy/battery/).

## NixOS

The flake exposes `nixosModules.default`:

```nix
{
  imports = [ inputs.hafox.nixosModules.default ];

  services.hafox = {
    enable = true;
    # package = inputs.hafox.packages.${pkgs.system}.default;
    # smartfoxUrl = "http://smartfox";
    # refreshInterval = "5s";
    # logFilter = "hafox=info";

    mqtt = {
      # host = "localhost";
      # port = 1883;
      # clientId = "hafox";
      # username = "hafox";
      # passwordFile = "/run/secrets/hafox-mqtt-password";
      # password = "insecure-example";
      # discoveryPrefix = "homeassistant";
      # topicPrefix = "hafox";
    };
  };
}
```

If the MQTT broker requires authentication, set `mqtt.username` and exactly one of `mqtt.passwordFile` or `mqtt.password`. Use `mqtt.passwordFile` for production secrets. `mqtt.password` stores the value in the Nix configuration.

## Credits

This project owes a debt to <https://github.com/michaelherger/homeassistant-smartfox>, which did a great job of collecting and extracting all the necessary information.
