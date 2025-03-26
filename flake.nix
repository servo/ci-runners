{
  inputs = {
    # Fix qemu crash on macOS guests (NixOS/nixpkgs#338598).
    # See also: <https://gitlab.com/qemu-project/qemu/-/commit/a8e63ff289d137197ad7a701a587cc432872d798>
    # Last version deployed before flakes was 68e7dce0a6532e876980764167ad158174402c6f.
    unstable.url = "github:NixOS/nixpkgs/8ce7f9f78bdbe659a8d7c1fe376b89b3a43e4cdc";
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
      monitorCrate = monitor.rootCrate.build;
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
