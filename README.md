# AdoBoards
A sleek, fast, and terminal based board manager built with Rust.

TODO: Add GIF with some dummy data.

## Features
* Browse work items
* Edit work items
* Filter work items by id / title
* Filter work items "Assigned to me"
* Refresh backlog
* See work item details
* Read multiple backlogs
* Open configuration in $EDITOR

## 🔑 Authentication

This project requires access to your Azure DevOps instance. You must authenticate using **one** of the following two methods:

### Option 1: Azure CLI 
The application can use your active Azure CLI session. Ensure you have the [Azure CLI installed](https://learn.microsoft.com/en-us/cli/azure/install-azure-cli) and are logged in:

```bash
az login
```

### Option 2: Personal Access Token (PAT)
If you prefer not to use the CLI, set your PAT as an environment variable.
Generate a PAT: Go to Azure DevOps -> User Settings -> Personal Access Tokens.
Required Scopes: Ensure the token has read permissions for Work Items.

Set Environment Variable:

Linux / macOS:
```bash
export AZURE_DEVOPS_EXT_PAT="your_token_here"
```

Windows (PowerShell):
```powershell
$env:AZURE_DEVOPS_EXT_PAT = "your_token_here"
```

## ⚙️ Configuration
On first run, adoboards will create a default configuration file for you. If no boards are configured, it will automatically open the file in your default $EDITOR.
Locations:
* Linux: ~/.config/adoboards/default-config.toml
* macOS: ~/Library/Application Support/adoboards/default-config.toml
* Windows: %APPDATA%\adoboards\default-config.toml
After editing the config file adoboards will automatically exit. Relaunch it so the new configuration takes place.

### Common
`me` should the the your name in the `displayName` format used in your ADO boards

### Boards
Boards are configured with:
```toml
[[boards]]
organization = "<organization>"
project = "<project>"
team = "<team>" // Usually "<project Team>"
```

The values can be found from the URL:
`https://dev.azure.com/<organization>/<project>`

### ⌨️ Hotkeys

Hotkeys are configurable. The default keys are:

### List View
| Name | Key | Action |
|------|-----|--------|
| quit | `q` / `Esc` | Quit adoboards |
| next | `j` / `↓` | Next item |
| previous | `k` / `↑` | Previous item |
| jump_to_top | `gg` | First item |
| jump_to_end | `G` | Last item |
|| `Enter` | Open selected item |
| hover | `K` | Open "hover" showing more information |
| refresh | `r` | Reload board |
| edit_config | `c` | Open configuration file with $EDITOR |
| next_board | `>` | Next board |
| previous_board | `<` | Previous board |
| search | `/` | Open filter |
| open | `o` | Open item in browser |
| assigned_to_me_filter | `m` | Toggle "assigned to me" filter |

### Item View
| Name | Key | Action |
|------|-----|--------|
| quit | `q` | Close item |
| open | `o` | Open item in browser |
| edit | `e` | Edit item |

---

## 🚀 Installation

### Prerequisites
On WSL `wslu` (or some other tool for `xdg-open` support) is needed for opening the work items in browser.

### ❄️ Using Nix (Recommended)
If you have Nix installed with Flakes enabled, you can run adoboards without even installing it:
```bash
nix run github:Wotee/adoboards-tui
```
    
To install it permanently to your profile:
```bash
nix profile install github:Wotee/adoboards-tui
```

### 🦀 Using Cargo
Make sure you have pkg-config and openssl development headers installed on your system, then run:
```bash
cargo install --path .
```

### 🏠 Home Manager Setup
Add adoboards to your declarative configuration for the ultimate Nix experience.

Add to flake.nix inputs:
```nix
adoboards = {
    url = "github:Wotee/adoboards-tui";
    inputs.nixpkgs.follows = "nixpkgs";
};
```
add adoboads to flake.nix outputs:
```nix
outputs = {
    nixpkgs,
    home-manager,
    adoboards,
    ...
};
```

then add extraSpecialArgs to your `homeManagerConfiguration`
```nix
"example" = home-manager.lib.homeManagerConfiguration {
    inherit pkgs;
    extraSpecialArgs = { inherit adoboards; };
    modules = [
        ...
```

Add to home.packages:
```nix
{
  config,
  pkgs,
  adoboards, # Add this one!
  lib,
  ...
}
```
and finally
```nix
home.packages = [
  inputs.adoboards.packages.${pkgs.system}.default
];
```

## 🛠️ Development

Use **Nix Flakes** and **direnv** to ensure a perfectly reproducible development environment.

## 🗺️ Roadmap

Future plans and ideas for `adoboards`:
### Common
* Configurable "backlog level"
* View hotkeys (for current view)
* Crashes if not authenticated
### List view
* Cache backlogs when using multiple?
* See iteration backlogs
* Create work items
### Detail view
* Refine WI description/AC so line breaks etc. are not broken
* See parent/child items 
* Go to parent/child items 


