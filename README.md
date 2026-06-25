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



## Usage

```sh
hafox dump --smartfox-url http://smartfox
hafox export --smartfox-url http://smartfox --mqtt-host myserver
hafox run --smartfox-url http://smartfox --mqtt-host myserver --refresh-interval 30s
```

`dump` prints the current normalized snapshot. `export` publishes Home Assistant MQTT discovery and one retained state update. `run` publishes discovery on the first successful update and then refreshes MQTT state continuously.

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
