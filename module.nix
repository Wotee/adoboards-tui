{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.adoboards;

  boardType = types.submodule {
    options = {
      organization = mkOption {
        type = types.str;
        default = "<organization>";
        description = "Azure DevOps organization name";
      };

      project = mkOption {
        type = types.str;
        default = "<project>";
        description = "Azure DevOps project name";
      };

      team = mkOption {
        type = types.str;
        default = "<project> Team";
        description = "Azure DevOps team name";
      };
    };
  };

  keysType = types.submodule {
    options = {
      quit = mkOption { type = types.str; default = "q"; description = "Quit key"; };
      next = mkOption { type = types.str; default = "j"; description = "Next item key"; };
      previous = mkOption { type = types.str; default = "k"; description = "Previous item key"; };
      hover = mkOption { type = types.str; default = "K"; description = "Hover key"; };
      open = mkOption { type = types.str; default = "o"; description = "Open in browser key"; };
      next_board = mkOption { type = types.str; default = ">"; description = "Next board key"; };
      previous_board = mkOption { type = types.str; default = "<"; description = "Previous board key"; };
      search = mkOption { type = types.str; default = "/"; description = "Search key"; };
      assigned_to_me_filter = mkOption { type = types.str; default = "m"; description = "Filter by me key"; };
      jump_to_top = mkOption { type = types.str; default = "gg"; description = "Jump to top key"; };
      jump_to_end = mkOption { type = types.str; default = "G"; description = "Jump to end key"; };
      refresh = mkOption { type = types.str; default = "r"; description = "Refresh key"; };
      edit_config = mkOption { type = types.str; default = "c"; description = "Edit config key"; };
    };
  };

  generateConfig = boards: me: keys: pkgs.writeTextFile {
    name = "default-config.toml";
    text = ''
      [common]
      me = "${me}"

      [keys]
      quit = "${keys.quit}"
      next = "${keys.next}"
      previous = "${keys.previous}"
      hover = "${keys.hover}"
      open = "${keys.open}"
      next_board = "${keys.next_board}"
      previous_board = "${keys.previous_board}"
      search = "${keys.search}" cfg.me cfg.keys
      assigned_to_me_filter = "${keys.assigned_to_me_filter}"
      jump_to_top = "${keys.jump_to_top}"
      jump_to_end = "${keys.jump_to_end}"
      refresh = "${keys.refresh}"
      edit_config = "${keys.edit_config}"

    '' + concatMapStringsSep "\n" (board: ''
      [[boards]]
      organization = "${board.organization}"
      project = "${board.project}"
      team = "${board.team}"
    '') boards;
  };

in {
  options.programs.adoboards = {
    enable = mkEnableOption "adoboards TUI";

    me = mkOption {
      type = types.str;
      default = config.home.username;
      description = "Your displayName as shown in Azure DevOps";
    };

    keys = mkOption {
      type = keysType;
      default = {};
      description = "Keyboard shortcuts configuration";
    };

    boards = mkOption {
      type = types.listOf boardType;
      default = [{
        organization = "<organization>";
        project = "<project>";
        team = "<project> Team";
      }];
      description = "List of Azure DevOps boards to configure";
    };

    configFile = mkOption {
      type = types.path;
      readOnly = true;
      description = "Generated config file path";
    };
  };

  config = mkIf cfg.enable {
    programs.adoboards.configFile = generateConfig cfg.boards;

    home.file.".config/adoboards/default-config.toml" = mkIf pkgs.stdenv.isLinux {
      source = cfg.configFile;
    };

    home.file."Library/Application Support/adoboards/default-config.toml" = mkIf pkgs.stdenv.isDarwin {
      source = cfg.configFile;
    };
  };
}
