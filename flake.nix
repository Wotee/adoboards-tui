{
  description = "Adoboards TUI";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        
        # Config generation script
        generateConfigScript = pkgs.writeShellScriptBin "adoboards-generate-config" ''
          #!/usr/bin/env bash
          set -euo pipefail

          # Determine config directory based on OS
          if [[ "$OSTYPE" == "darwin"* ]]; then
              CONFIG_DIR="$HOME/Library/Application Support/adoboards"
          elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
              CONFIG_DIR="''${XDG_CONFIG_HOME:-$HOME/.config}/adoboards"
          else
              echo "Unsupported OS: $OSTYPE (Nix only supports Linux and macOS)"
              exit 1
          fi

          CONFIG_FILE="$CONFIG_DIR/default-config.toml"

          # Parse arguments
          ME="''${1:-$USER}"
          ORGANIZATION="''${2:-<organization>}"
          PROJECT="''${3:-<project>}"
          TEAM="''${4:-<project> Team}"

          # Create config directory if it doesn't exist
          mkdir -p "$CONFIG_DIR"

          # Generate config file
          cat > "$CONFIG_FILE" <<EOF
          [common]
          me = "$ME"

          [keys]
          quit = "q"
          next = "j"
          previous = "k"
          hover = "K"
          open = "o"
          next_board = ">"
          previous_board = "<"
          search = "/"
          assigned_to_me_filter = "m"
          jump_to_top = "gg"
          jump_to_end = "G"
          refresh = "r"
          edit_config = "c"

          [[boards]]
          organization = "$ORGANIZATION"
          project = "$PROJECT"
          team = "$TEAM"
          EOF

          echo "Generated config at: $CONFIG_FILE"
        '';
        
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "adoboards";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [
            pkgs.pkg-config
          ];

          buildInputs = [
            pkgs.openssl
          ];
        };

        packages.config-generator = generateConfigScript;

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            generateConfigScript
          ];
          inputsFrom = [ self.packages.${system}.default ];
          
          shellHook = ''
            echo "Adoboards development shell"
            echo "Run 'adoboards-generate-config [me] [org] [project] [team]' to create config"
          '';
        };

        # Home Manager module export
        homeManagerModules.default = import ./module.nix;
      }
    );
}
