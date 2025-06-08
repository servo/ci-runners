{
  monitorCrate,
  image-deps,

  fetchurl,
  lib,
  stdenv,
  makeWrapper,

  cdrkit,
  gawk,
  gh,
  git,
  gnused,
  jq,
  libvirt,
  openssh,
  time,
  virt-manager,
  zfs,
  zsh,
}: let
  fs = lib.fileset;

  sources = fs.intersection (fs.gitTracked ../..) (
    fs.unions [
      ../../monitor
      ../../shared
      ../../macos13
      ../../ubuntu2204
      ../../ubuntu2204-rust
      ../../ubuntu2204-wpt
      ../../windows10
      ../../common.sh
      ../../register-runner.sh
      ../../reserve-runner.sh
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

  nativeBuildInputs = [
    makeWrapper
  ];

  buildInputs = [
    monitorCrate
    cdrkit  # for genisoimage(1)
    gawk  # for awk(1)
    gh
    git
    gnused  # for sed(1)
    jq
    libvirt  # for virsh(1)
    openssh  # for ssh(1)
    time  # for time(1)
    virt-manager  # for virt-clone(1)
    zfs
    zsh
    # TODO: list other commands needed by scripts here
  ];

  # Some of the scripts run in the guest, like {macos13,ubuntu2204}/init.sh.
  # Don’t patch shebangs, otherwise those scripts will be broken.
  dontPatchShebangs = true;

  installPhase = ''
    mkdir -p $out/bin
    ln -s ${monitorCrate}/bin/monitor $out/bin/monitor
    cd ..  # cd back out of sourceRoot
    mkdir -p $out/lib/monitor
    cp -R shared $out/lib/monitor
    cp -R macos13 $out/lib/monitor
    cp -R ubuntu2204 $out/lib/monitor
    cp -R ubuntu2204-rust $out/lib/monitor
    cp -R ubuntu2204-wpt $out/lib/monitor
    cp -R windows10 $out/lib/monitor
    cp -R common.sh $out/lib/monitor
    cp -R register-runner.sh $out/lib/monitor
    cp -R reserve-runner.sh $out/lib/monitor
  '';

  postFixup = ''
    wrapProgram $out/bin/monitor --set PATH ${lib.makeBinPath buildInputs} \
      --set LIB_MONITOR_DIR $out/lib/monitor \
      --set IMAGE_DEPS_DIR ${image-deps}
  '';
}
