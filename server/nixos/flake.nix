{
  inputs = {
    # Fix qemu crash on macOS guests (NixOS/nixpkgs#338598).
    # See also: <https://gitlab.com/qemu-project/qemu/-/commit/a8e63ff289d137197ad7a701a587cc432872d798>
    # Last version deployed before flakes was 68e7dce0a6532e876980764167ad158174402c6f.
    unstable.url = "github:NixOS/nixpkgs/8ce7f9f78bdbe659a8d7c1fe376b89b3a43e4cdc";
  };

  outputs = inputs@{ self, unstable, ... }:
  let
    pkgsUnstable = import unstable {
      system = "x86_64-linux";
      config = { allowUnfree = true; };
    };
  in {
    nixosConfigurations.ci0 = unstable.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [ ./configuration.nix ];
    };
    packages.x86_64-linux.monitor = pkgsUnstable.callPackage ./monitor.nix {};
  };
}
