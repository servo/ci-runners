{
  inputs = {
    unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    crate2nix.url = "github:nix-community/crate2nix";
    crate2nix.inputs.nixpkgs.follows = "unstable";
  };

  outputs = inputs@{ self, unstable, ... }:
  let
    forEachSystem = fn:
      unstable.lib.genAttrs [
        "x86_64-linux"
        "aarch64-darwin"
      ] (system: fn system (
        import unstable {
          inherit system;
          config.allowUnfree = true;
        }
      ));
    monitor = inputs.crate2nix.tools.x86_64-linux.appliedCargoNix {
      name = "monitor";
      src = ./.;
    };
  in {
    nixosConfigurations.ci0 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci0";
        hostId = "04AA04E2";
        ipv6Address = "2a01:4f9:3071:3063::2";
        hugepages = 96;
        hasIntermittentTracker = true;
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    nixosConfigurations.ci1 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci1";
        hostId = "47264830";
        ipv6Address = "2a01:4f9:3100:1d2b::2";
        hugepages = 96;
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    nixosConfigurations.ci2 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci2";
        hostId = "A2BB6C74";
        ipv6Address = "2a01:4f9:3100:1963::2";
        hugepages = 96;
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    nixosConfigurations.ci3 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci3";
        hostId = "51A83C6A";
        ipv6Address = "2a01:4f9:6a:555e::2";
        hugepages = 24;
        isBenchmarkingMachine = true;
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    nixosConfigurations.ci4 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci4";
        hostId = "E76BDFD4";
        ipv6Address = "2a01:4f9:6a:4d27::2";
        hugepages = 24;
        isBenchmarkingMachine = true;
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    packages = forEachSystem (
      system: pkgs: {
        monitor = pkgs.callPackage server/nixos/monitor.nix {
          monitorCrate = monitor.workspaceMembers.monitor.build;
          image-deps = self.packages.${system}.image-deps;
        };
        image-deps = pkgs.callPackage server/nixos/image-deps.nix {};
      }
    );
    devShells = forEachSystem (
      system: pkgs: {
        default = import ./shell.nix {
          inherit pkgs;
          inherit (self.packages.${system}) image-deps monitor;
        };
      }
    );
  };
}
