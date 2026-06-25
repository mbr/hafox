# hafox

`hafox` reads the local SmartFox `values.xml` endpoint and publishes a normalized model.

## Usage

```sh
hafox dump --smartfox-url http://smartfox
hafox export --smartfox-url http://smartfox --mqtt-host astarion
hafox run --smartfox-url http://smartfox --mqtt-host astarion --refresh-interval 30s
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
    refreshInterval = "30s";
    environmentFile = "/run/secrets/hafox.env";

    mqtt = {
      host = "astarion";
      username = "hafox";
    };
  };
}
```

The environment file may contain `HAFOX_MQTT_PASSWORD`, or both MQTT credential variables.
