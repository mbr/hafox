{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-26.05";
    fenix = {
      url = "fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        toolchain = fenix.packages.${system}.stable.withComponents [
          "cargo"
          "clippy"
          "rust-analyzer"
          "rust-src"
          "rustc"
          "rustfmt"
        ];

        platform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        cargoToml = pkgs.lib.importTOML ./Cargo.toml;

        rustEnv = {
          RUSTFLAGS = pkgs.lib.optionalString pkgs.stdenv.isLinux "-Clink-self-contained=-linker";
        };
      in
      {
        packages.default = platform.buildRustPackage (
          rustEnv
          // rec {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;
            description = cargoToml.package.description;
            nativeBuildInputs = with pkgs; [ llvmPackages.bintools ];

            src = pkgs.lib.cleanSource ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            meta.mainProgram = pname;
          }
        );

        devShells.default = pkgs.mkShell (
          rustEnv
          // {
            inputsFrom = [ self.packages.${system}.default ];
            buildInputs = [ pkgs.nixfmt ];
            RUST_LOG = "debug";
          }
        );
      }
    )
    // {
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.services.hafox;

          serviceEnvironment = {
            HAFOX_SMARTFOX_URL = cfg.smartfoxUrl;
            HAFOX_MQTT_HOST = cfg.mqtt.host;
            HAFOX_MQTT_PORT = toString cfg.mqtt.port;
            HAFOX_MQTT_CLIENT_ID = cfg.mqtt.clientId;
            HAFOX_MQTT_DISCOVERY_PREFIX = cfg.mqtt.discoveryPrefix;
            HAFOX_MQTT_TOPIC_PREFIX = cfg.mqtt.topicPrefix;
            HAFOX_REFRESH_INTERVAL = cfg.refreshInterval;
            RUST_LOG = cfg.logFilter;
          }
          // lib.optionalAttrs (cfg.mqtt.username != null) {
            HAFOX_MQTT_USERNAME = cfg.mqtt.username;
          };
        in
        {
          options.services.hafox = {
            enable = lib.mkEnableOption "hafox SmartFox to Home Assistant MQTT bridge";

            package = lib.mkPackageOption self.packages.${pkgs.stdenv.hostPlatform.system} "default" { };

            smartfoxUrl = lib.mkOption {
              type = lib.types.str;
              default = "http://smartfox";
              description = "SmartFox web interface base URL.";
            };

            refreshInterval = lib.mkOption {
              type = lib.types.str;
              default = "5s";
              description = "Delay between SmartFox updates.";
            };

            logFilter = lib.mkOption {
              type = lib.types.str;
              default = "hafox=info";
              description = "Tracing filter used by the service.";
            };

            environmentFile = lib.mkOption {
              type = lib.types.nullOr (lib.types.either lib.types.path lib.types.str);
              default = null;
              description = "Environment file containing secret service variables.";
            };

            mqtt = {
              host = lib.mkOption {
                type = lib.types.str;
                default = "localhost";
                description = "MQTT broker host name or address.";
              };

              port = lib.mkOption {
                type = lib.types.port;
                default = 1883;
                description = "MQTT broker TCP port.";
              };

              clientId = lib.mkOption {
                type = lib.types.str;
                default = "hafox";
                description = "MQTT client identifier.";
              };

              username = lib.mkOption {
                type = lib.types.nullOr lib.types.str;
                default = null;
                description = "MQTT user name.";
              };

              discoveryPrefix = lib.mkOption {
                type = lib.types.str;
                default = "homeassistant";
                description = "Home Assistant MQTT discovery topic prefix.";
              };

              topicPrefix = lib.mkOption {
                type = lib.types.str;
                default = "hafox";
                description = "MQTT state topic prefix.";
              };
            };
          };

          config = lib.mkIf cfg.enable {
            systemd.services.hafox = {
              description = "hafox SmartFox to Home Assistant MQTT bridge";
              wantedBy = [ "multi-user.target" ];
              wants = [ "network-online.target" ];
              after = [ "network-online.target" ];
              environment = serviceEnvironment;

              serviceConfig = {
                ExecStart = "${lib.getExe cfg.package} run";
                Restart = "always";
                RestartSec = "5s";
                DynamicUser = true;
                LockPersonality = true;
                MemoryDenyWriteExecute = true;
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectControlGroups = true;
                ProtectHome = true;
                ProtectKernelModules = true;
                ProtectKernelTunables = true;
                ProtectSystem = "strict";
                RestrictAddressFamilies = [
                  "AF_INET"
                  "AF_INET6"
                  "AF_UNIX"
                ];
                RestrictSUIDSGID = true;
                SystemCallArchitectures = "native";
              }
              // lib.optionalAttrs (cfg.environmentFile != null) {
                EnvironmentFile = cfg.environmentFile;
              };
            };
          };
        };
    };
}
