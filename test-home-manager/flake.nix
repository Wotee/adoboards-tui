{
  description = "Test flake for adoboards Home Manager module";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    adoboards.url = "path:..";
  };

  outputs = { self, nixpkgs, home-manager, adoboards }:
    let
      system = "aarch64-darwin";  # Change to x86_64-darwin if on Intel Mac
      pkgs = nixpkgs.legacyPackages.${system};
      username = builtins.getEnv "USER";
    in
    {
      homeConfigurations.test = home-manager.lib.homeManagerConfiguration {
        inherit pkgs;
        
        modules = [
          adoboards.homeManagerModules.default
          {
            home.username = username;
            home.homeDirectory = "/Users/${username}";
            home.stateVersion = "24.05";

            # Test configuration with single board
            programs.adoboards = {
              enable = true;
              me = "Test User";
              boards = [
                {
                  organization = "test-org";
                  project = "test-project";
                  team = "test-project Team";
                }
              ];
            };
          }
        ];
      };

      homeConfigurations.test-multi = home-manager.lib.homeManagerConfiguration {
        inherit pkgs;
        
        modules = [
          adoboards.homeManagerModules.default
          {
            home.username = username;
            home.homeDirectory = "/Users/${username}";
            home.stateVersion = "24.05";

            # Test configuration with multiple boards
            programs.adoboards = {
              enable = true;
              boards = [
                {
                  organization = "org1";
                  project = "project1";
                  team = "Project 1 Team";
                }
                {
                  organization = "org2";
                  project = "project2";
                  team = "Project 2 Team";
                }
              ];
            };
          }
        ];
      };

      homeConfigurations.test-defaults = home-manager.lib.homeManagerConfiguration {
        inherit pkgs;
        
        modules = [
          adoboards.homeManagerModules.default
          {
            home.username = username;
            home.homeDirectory = "/Users/${username}";
            home.stateVersion = "24.05";

            # Test with defaults only
            programs.adoboards.enable = true;
          }
        ];
      };
    };
}
