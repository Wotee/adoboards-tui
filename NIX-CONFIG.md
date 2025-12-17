# Nix Configuration Management

## Overview
Declarative configuration for adoboards using Nix/Home Manager/nix-darwin modules.

## Declarative Configuration (Recommended)

### Home Manager

Add to your flake inputs:
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    home-manager.url = "github:nix-community/home-manager";
    adoboards.url = "github:yourusername/adoboards-tui";  # or "path:/path/to/adoboards-tui"
  };

  outputs = { self, nixpkgs, home-manager, adoboards, ... }: {
    homeConfigurations.youruser = home-manager.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        adoboards.homeManagerModules.default
        {
          programs.adoboards = {
            enable = true;
            me = "John Doe";  # Optional: defaults to username
            # keys = { ... };  # Optional: customize keyboard shortcuts
            boards = [
              {
                organization = "myorg";
                project = "myproject";
                team = "myproject Team";
              }
              {
                organization = "myorg";
                project = "anotherproject";
                team = "anotherproject Team";
              }
            ];
          };
        }
      ];
    };
  };
}
```

### nix-darwin

Add to your Darwin configuration:
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    darwin.url = "github:lnl7/nix-darwin";
    adoboards.url = "github:yourusername/adoboards-tui";
  };

  outputs = { self, nixpkgs, darwin, adoboards, ... }: {
    darwinConfigurations.yourhostname = darwin.lib.darwinSystem {
      modules = [
        adoboards.homeManagerModules.default
        {
          programs.adoboards.enable = true;
          programs.adoboards.boards = [
            {
              organization = "myorg";
              project = "myproject";
              team = "myproject Team";
            }
          ];
        }
      ];
    };
  };
}
```

### Configuration Options

- `programs.adoboards.enable` - Enable adoboards configuration (default: `false`)
- `programs.adoboards.me` - Your displayName in Azure DevOps (default: `config.home.username`)
- `programs.adoboards.keys` - Keyboard shortcuts (see below for defaults)
- `programs.adoboards.boards` - List of board configurations
  - `organization` - Azure DevOps organization (default: `"<organization>"`)
  - `project` - Azure DevOps project (default: `"<project>"`)
  - `team` - Azure DevOps team (default: `"<project> Team"`)

#### Default Keyboard Shortcuts
```nix
keys = {
  quit = "q";
  next = "j";
  previous = "k";
  hover = "K";
  open = "o";
  next_board = ">";
  previous_board = "<";
  search = "/";
  assigned_to_me_filter = "m";
  jump_to_top = "gg";
  jump_to_end = "G";
  refresh = "r";
  edit_config = "c";
};
```

## Imperative Configuration (Development)

### Development Shell
Enter the development shell:
```bash
nix develop
```

Generate a config file with defaults:
```bash
adoboards-generate-config
```

Generate with specific values:
```bash
adoboards-generate-config "myorg" "myproject" "myproject Team"
```

### Standalone Script
Run the config generator directly:
```bash
nix run .#config-generator -- "myorg" "myproject" "myproject Team"
```

## Config File Location
- **Linux**: `~/.config/adoboards/default-config.toml`
- **macOS**: `~/Library/Application Support/adoboards/default-config.toml`

## Config Format
```toml
[[boards]]
organization = "<organization>"
project = "<project>"
team = "<team>"
```

Multiple boards can be added by repeating the `[[boards]]` section.
