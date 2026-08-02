{
  description = "PIDRA - keyboard-first Linux terminal process manager";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;

      mkPidra = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "pidra";
          version = "0.1.0";

          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # The integration tests spawn /usr/bin/sleep and may use a running
          # systemd user manager, neither of which is guaranteed in a Nix
          # build sandbox. Keep the deterministic library tests in the build.
          cargoTestFlags = [ "--lib" ];

          meta = {
            description = "Keyboard-first Linux terminal process manager";
            homepage = "https://github.com/mika2go/PIDRA";
            mainProgram = "pidra";
            platforms = pkgs.lib.platforms.linux;
          };
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
          pidra = mkPidra pkgs;
        in
        {
          inherit pidra;
          default = pidra;
        }
      );

      apps = forAllSystems (
        system:
        let
          package = self.packages.${system}.pidra;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/pidra";
          };
        }
      );

      overlays.default = final: _prev: {
        pidra = mkPidra final;
      };

      nixosModules = {
        default = self.nixosModules.pidra;

        pidra =
          { config, lib, pkgs, ... }:
          let
            cfg = config.programs.pidra;
          in
          {
            options.programs.pidra = {
              enable = lib.mkEnableOption "PIDRA, the terminal process manager";

              package = lib.mkOption {
                type = lib.types.package;
                default = mkPidra pkgs;
                description = "The PIDRA package to install.";
              };
            };

            config = lib.mkIf cfg.enable {
              environment.systemPackages = [ cfg.package ];
            };
          };
      };

      homeManagerModules = {
        default = self.homeManagerModules.pidra;

        pidra =
          { config, lib, pkgs, ... }:
          let
            cfg = config.programs.pidra;
          in
          {
            options.programs.pidra = {
              enable = lib.mkEnableOption "PIDRA, the terminal process manager";

              package = lib.mkOption {
                type = lib.types.package;
                default = mkPidra pkgs;
                description = "The PIDRA package to install.";
              };
            };

            config = lib.mkIf cfg.enable {
              home.packages = [ cfg.package ];
            };
          };
      };
    };
}
