{
  inputs = {
    unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    crate2nix.url = "github:nix-community/crate2nix";
    crate2nix.inputs.nixpkgs.follows = "unstable";
  };

  outputs = inputs@{ self, unstable, ... }:
  let
    pkgsUnstable = import unstable {
      system = "x86_64-linux";
      config = { allowUnfree = true; };
    };
    monitor = inputs.crate2nix.tools.x86_64-linux.appliedCargoNix {
      name = "monitor";
      src = ./monitor;
    };
  in {
    nixosConfigurations.ci0 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci0";
        hostId = "04AA04E2";
        ipv6Address = "2a01:4f9:3071:3063::2";
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
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    nixosConfigurations.ci2 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ (import server/nixos/configuration.nix {
        hostName = "ci2";
        hostId = "A2BB6C74";
        ipv6Address = "2a01:4f9:3100:1963::2";
        monitor = self.packages.x86_64-linux.monitor;
      }) ];
    };
    packages.x86_64-linux.monitor = pkgsUnstable.callPackage server/nixos/monitor.nix {
      monitorCrate = monitor.workspaceMembers.monitor.build;
      image-deps = self.packages.x86_64-linux.image-deps;
    };
    packages.x86_64-linux.image-deps = pkgsUnstable.callPackage server/nixos/image-deps.nix {};
    devShells.x86_64-linux.default = import ./shell.nix {
      inherit (pkgsUnstable) pkgs;
      image-deps = self.packages.x86_64-linux.image-deps;
      monitor = self.packages.x86_64-linux.monitor;
    };
  };
}
