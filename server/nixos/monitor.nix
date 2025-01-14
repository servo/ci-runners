{ lib, rustPlatform, makeWrapper, zsh }: let
  fs = lib.fileset;

  sources = fs.intersection (fs.gitTracked ../..) (
    fs.unions [
      ../../monitor
      ../../macos13
      ../../ubuntu2204
      ../../windows10
      ../../common.sh
      ../../create-runner.sh
      ../../destroy-runner.sh
      ../../download.sh
      ../../inject.sh
      ../../list-libvirt-guests.sh
      ../../list-registered-runners.sh
      ../../list-runner-volumes.sh
      ../../mount-runner.sh
      ../../register-runner.sh
      ../../reserve-runner.sh
      ../../unregister-runner.sh
    ]
  );
in rustPlatform.buildRustPackage rec {
  pname = "monitor";
  version = "0.1.0";

  src = fs.toSource {
    root = ../..;
    fileset = sources;
  };
  sourceRoot = "/build/source/monitor";

  # don't forget to update this hash when Cargo.lock or ${version} changes!
  cargoHash = "sha256-pm5plXdFstpHr01ORspGXtGBT1e693zAlnRO88LdGRM=";

  postConfigure = ''
    export LIB_MONITOR_DIR=$out/lib/monitor
  '';

  nativeBuildInputs = [
    makeWrapper
  ];

  buildInputs = [
    zsh
    # TODO: list other commands needed by scripts here
  ];

  postInstall = ''
    cd ..  # cd back out of sourceRoot
    mkdir -p $out/lib/monitor
    cp -R macos13 $out/lib/monitor
    cp -R ubuntu2204 $out/lib/monitor
    cp -R windows10 $out/lib/monitor
    cp -R common.sh $out/lib/monitor
    cp -R create-runner.sh $out/lib/monitor
    cp -R destroy-runner.sh $out/lib/monitor
    cp -R download.sh $out/lib/monitor
    cp -R inject.sh $out/lib/monitor
    cp -R list-libvirt-guests.sh $out/lib/monitor
    cp -R list-registered-runners.sh $out/lib/monitor
    cp -R list-runner-volumes.sh $out/lib/monitor
    cp -R mount-runner.sh $out/lib/monitor
    cp -R register-runner.sh $out/lib/monitor
    cp -R reserve-runner.sh $out/lib/monitor
    cp -R unregister-runner.sh $out/lib/monitor
  '';

  postFixup = ''
    wrapProgram $out/bin/monitor --set PATH ${lib.makeBinPath buildInputs}
  '';
}
