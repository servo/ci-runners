{
  monitorCrate,

  callPackage,
  fetchurl,
  lib,
  stdenv,
  makeWrapper,
  gawk,
  git,
  gnused,
  libvirt,
  openssh,
  zsh,
}: let
  fs = lib.fileset;

  sources = fs.intersection (fs.gitTracked ../..) (
    fs.unions [
      ../../monitor
      ../../shared
      ../../macos13
      ../../ubuntu2204
      ../../ubuntu2204-wpt
      ../../windows10
      ../../common.sh
      ../../create-runner.sh
      ../../destroy-runner.sh
      ../../download.sh
      ../../inject.sh
      ../../list-registered-runners.sh
      ../../list-runner-volumes.sh
      ../../mount-runner.sh
      ../../register-runner.sh
      ../../reserve-runner.sh
      ../../unregister-runner.sh
    ]
  );
in stdenv.mkDerivation rec {
  pname = "monitor";
  version = "0.1.0";

  src = fs.toSource {
    root = ../..;
    fileset = sources;
  };
  sourceRoot = "/build/source/monitor";

  # don't forget to update this hash when Cargo.lock or ${version} changes!
  cargoHash = "sha256-cSQLIwhgkfihKOLzg9LD/1GTkNSyBJaDDTdfu5IzvdI=";

  postConfigure = ''
    export LIB_MONITOR_DIR=$out/lib/monitor
    export IMAGE_DEPS_DIR=${callPackage ./image-deps.nix {}}
  '';

  nativeBuildInputs = [
    makeWrapper
  ];

  buildInputs = [
    monitorCrate
    gawk
    git
    gnused
    libvirt
    openssh
    zsh
    # TODO: list other commands needed by scripts here
  ];

  # Some of the scripts run in the guest, like {macos13,ubuntu2204}/init.sh.
  # Don’t patch shebangs, otherwise those scripts will be broken.
  dontPatchShebangs = true;

  installPhase = ''
    mkdir -p $out/bin
    ln -s ${monitorCrate}/bin/monitor $out/bin/monitor
  '';

  postInstall = ''
    cd ..  # cd back out of sourceRoot
    mkdir -p $out/lib/monitor
    cp -R shared $out/lib/monitor
    cp -R macos13 $out/lib/monitor
    cp -R ubuntu2204 $out/lib/monitor
    cp -R ubuntu2204-wpt $out/lib/monitor
    cp -R windows10 $out/lib/monitor
    cp -R common.sh $out/lib/monitor
    cp -R create-runner.sh $out/lib/monitor
    cp -R destroy-runner.sh $out/lib/monitor
    cp -R download.sh $out/lib/monitor
    cp -R inject.sh $out/lib/monitor
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
