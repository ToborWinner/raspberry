flake:
{ lib, pkgs, config, ... }:

let
  raspberry-pkg = flake.packages.${pkgs.stdenv.hostPlatform.system}.raspberry;
  cfg = config.services.raspberry;
in
{
  options.services.raspberry = {
    enable = lib.mkEnableOption "raspberry";
    package = lib.mkOption {
      type = lib.types.package;
      default = raspberry-pkg;
      description = "The package to use for the Raspberry voice assistant service";
      example = "inputs.raspberry.packages.default";
    };

    user = lib.mkOption {
      default = "raspberry";
      type = with lib.types; uniq string;
      description = ''
        Name of the user to run the service as.
      '';
    };

    voskModelName = lib.mkOption {
      default = "vosk-model-small-en-us-0.15";
      type = lib.types.string;
      description = "The vosk model to install in the configuration. Downloaded from alphacephei.com.";
    };
  };

  config =
    let
      voskModel = pkgs.fetchzip {
        url = "https://alphacephei.com/vosk/models/${cfg.voskModelName}.zip";
        hash = "sha256-CIoPZ/krX+UW2w7c84W3oc1n4zc9BBS/fc8rVYUthuY=";
      };
      embeddingModel = {
        config = pkgs.fetchurl {
          url = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/config.json";
          hash = "sha256-CU+OiRuTLyAAySz8ZjusTGIGn12K9bUnjEMGrvMIR1A=";
        }
        special_tokens_map = pkgs.fetchurl {
          url = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/special_tokens_map.json";
          hash = "sha256-ttNGvjZqfR1IMy28n987+JYLXYeVIrd5ndulnnYjfuM=";
        }
        tokenizer_config = pkgs.fetchurl {
          url = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer_config.json";
          hash = "sha256-kmHn15tEyBlcHK2itFPlWwCuuB6QemZkl0tNd3YXKrM=";
        }
        tokenizer = pkgs.fetchurl {
          url = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/tokenizer.json";
          hash = "sha256-0kGmDV6PBMwbKz6e96SSGye/Um2fYFCrkPkmeh+eXGY=";
        }
        model = pkgs.fetchurl {
          url = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/onnx/model.onnx";
          hash = "sha256-go4Ultf6u3nPpNzYT6OGJcDT0h2kdKAPCNsPVZlAzzU=";
        }
      };
      dataDir = "/etc/raspberry"
    in
    lib.mkIf cfg.enable {
      users.users.${cfg.user} = {
        description = "Raspberry Assistant daemon user";
        isSystemUser = true;
        group = cfg.user;
      };

      systemd.services.raspberry = {
        description = "Raspberry Assistant service";

        # TODO: Find a better one to ensure audio has already been set up
        after = [ "network-online.target" ];
        wantedBy = [ "multi-user.target" ];

        # https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html
        serviceConfig = {
          User = cfg.user;
          Group = cfg.user;
          Type = "exec"; # or simple maybe?
          ExecStart = "${cfg.package}/bin/raspberry";
          Restart = "no"; # or always and set RestartSec = 5
          ConfigurationDirectory = "raspberry"; # https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#RuntimeDirectory=

          # Add configuration - TODO: For embeddingModel use some nix functions
          preStart = ''
            ln -sfn ${voskModel} ${dataDir}/${voskModelName}
            mkdir -p /etc/raspberry/intents
            ln -sfn ${embeddingModel.config} ${dataDir}/intents/config.json
            ln -sfn ${embeddingModel.special_tokens_map} ${dataDir}/intents/special_tokens_map.json
            ln -sfn ${embeddingModel.tokenizer_config} ${dataDir}/intents/tokenizer_config.json
            ln -sfn ${embeddingModel.tokenizer} ${dataDir}/intents/tokenizer.json
            ln -sfn ${embeddingModel.model} ${dataDir}/intents/model.onnx
          '';

          # Hardening
          NoNewPrivileges = true;
          ProtectSystem = "strict";
          ProtectHome = true;
          ProtectClock = true;
          PrivateNetwork = true; # Network not needed for now
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectKernelLogs = true;
          LockPersonality = true;
          RemoveIPC = true;
          PrivateUsers = true;
        };
      };
    };
}
