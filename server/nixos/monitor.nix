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
  unzip,
  virt-manager,
  zfs,
  zsh,
}: let
  fs = lib.fileset;

  sources = fs.intersection (fs.gitTracked ../..) (
    fs.unions [
      ../../profiles
      ../../shared
    ]
  );
in stdenv.mkDerivation rec {
  pname = "monitor";
  version = "0.1.0";

  src = fs.toSource {
    root = ../..;
    fileset = sources;
  };

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
    unzip  # for funzip(1)
    virt-manager  # for virt-clone(1)
    zfs
    zsh
  ];

  # Some of the scripts run in the guest, like {macos13,ubuntu2204}/init.sh.
  # Don’t patch shebangs, otherwise those scripts will be broken.
  dontPatchShebangs = true;

  installPhase = ''
    mkdir -p $out/bin
    ln -s ${monitorCrate}/bin/monitor $out/bin/monitor
    ln -s ${monitorCrate}/bin/chunker $out/bin/chunker
    ln -s ${monitorCrate}/bin/queue $out/bin/queue
    mkdir -p $out/lib/monitor
    cp -R profiles $out/lib/monitor
    cp -R shared $out/lib/monitor
  '';

  postFixup = ''
    wrapProgram $out/bin/monitor --set PATH ${lib.makeBinPath buildInputs} \
      --set LIB_MONITOR_DIR $out/lib/monitor \
      --set IMAGE_DEPS_DIR ${image-deps}
    wrapProgram $out/bin/queue --set PATH ${lib.makeBinPath buildInputs} \
      --set LIB_MONITOR_DIR $out/lib/queue \
      --set IMAGE_DEPS_DIR ${image-deps}
  '';
}
