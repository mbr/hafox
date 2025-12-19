{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    (flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        pyproject = builtins.fromTOML (builtins.readFile ./pyproject.toml);

        pythonDeps =
          ps: with ps; [
            # Note: Python dependencies are managed by nix and should be
            #       added here instead of pyproject.toml.
            requests
            click
            rich
            python-dateutil
            tabulate
            paho-mqtt
          ];
      in
      {
        packages.default = pkgs.python3Packages.buildPythonApplication {
          pname = pyproject.project.name;
          version = pyproject.project.version;
          src = ./.;
          format = "pyproject";

          nativeBuildInputs = with pkgs.python3Packages; [
            setuptools
          ];

          propagatedBuildInputs = pythonDeps pkgs.python3Packages;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            (python3.withPackages pythonDeps)
            ruff
            mosquitto
          ];
        };
      }
    ))
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

          buildArgs = lib.concatStringsSep " " (
            lib.filter (s: s != "") [
              (lib.optionalString (cfg.host != null) "--host ${cfg.host}")
              (lib.optionalString (cfg.port != null) "--port ${toString cfg.port}")
              (lib.optionalString (cfg.username != null) "--username ${cfg.username}")
              (lib.optionalString (cfg.password != null) "--password ${cfg.password}")
              (lib.optionalString (cfg.interval != null) "--interval ${toString cfg.interval}")
              (lib.optionalString (!cfg.discovery) "--no-discovery")
              (lib.optionalString (cfg.topicPrefix != null) "--topic-prefix ${cfg.topicPrefix}")
              (lib.optionalString (cfg.deviceId != null) "--device-id ${cfg.deviceId}")
              (lib.optionalString (cfg.smartfoxUrl != null) "--smartfox-url ${cfg.smartfoxUrl}")
              (lib.optionalString (cfg.rebootInterval != null) "--reboot-interval ${toString cfg.rebootInterval}")
              (lib.optionalString cfg.verbose "--verbose")
            ]
          );
        in
        {
          options.services.hafox = {
            enable = lib.mkEnableOption "hafox SmartFox to MQTT bridge";

            host = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "MQTT broker host (required)";
            };

            port = lib.mkOption {
              type = lib.types.nullOr lib.types.port;
              default = null;
              description = "MQTT broker port";
            };

            username = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "MQTT username";
            };

            password = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "MQTT password";
            };

            interval = lib.mkOption {
              type = lib.types.nullOr lib.types.ints.positive;
              default = null;
              description = "Polling interval in seconds";
            };

            discovery = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Enable Home Assistant discovery";
            };

            topicPrefix = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "MQTT topic prefix";
            };

            deviceId = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Device identifier";
            };

            smartfoxUrl = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "SmartFox base URL";
            };

            rebootInterval = lib.mkOption {
              type = lib.types.nullOr lib.types.ints.positive;
              default = null;
              description = "Send device reboot request every N seconds (WARNING: reboots the device)";
            };

            verbose = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Enable verbose logging";
            };
          };

          config = lib.mkIf cfg.enable {
            assertions = [
              {
                assertion = cfg.host != null;
                message = "services.hafox.host must be set";
              }
            ];

            systemd.services.hafox = {
              description = "Hafox SmartFox to MQTT bridge";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];
              serviceConfig = {
                ExecStart = "${self.packages.${pkgs.system}.default}/bin/hafox publish ${buildArgs}";
                Restart = "on-failure";
                DynamicUser = true;
                # Security hardening
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "strict";
                ProtectHome = true;
              };
            };
          };
        };
    };
}
