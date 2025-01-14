{ lib, rustPlatform, makeWrapper, zsh }: let
  fs = lib.fileset;

  sources = fs.intersection (fs.gitTracked ../../monitor) (
    fs.unions [
      ../../monitor/Cargo.lock
      ../../monitor/Cargo.toml
      ../../monitor/src
      ../../monitor/templates
    ]
  );
in rustPlatform.buildRustPackage rec {
  pname = "monitor";
  version = "0.1.0";

  src = fs.toSource {
    root = ../../monitor;
    fileset = sources;
  };

  # don't forget to update this hash when Cargo.lock or ${version} changes!
  cargoHash = "sha256-pm5plXdFstpHr01ORspGXtGBT1e693zAlnRO88LdGRM=";

  nativeBuildInputs = [
    makeWrapper
  ];

  buildInputs = [
    zsh
    # TODO: list other commands needed by scripts here
  ];

  postFixup = ''
    wrapProgram $out/bin/monitor --set PATH ${lib.makeBinPath buildInputs}
  '';
}
