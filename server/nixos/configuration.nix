# Edit this configuration file to define what should be installed on
# your system. Help is available in the configuration.nix(5) man page, on
# https://search.nixos.org/options and in the NixOS manual (`nixos-help`).

{
  hostName,
  hostId,  # Generate with: LC_ALL=C < /dev/urandom tr -dC 0-9A-F | head -c 8
  ipv6Address,
  hugepages,
  isBenchmarkingMachine ? false,
  hasIntermittentTracker ? false,
  monitor,
}:

{ config, lib, pkgs, ... }:

{
  # hardware-configuration.nix
  boot.initrd.availableKernelModules = [ "nvme" "xhci_pci" "ahci" ];
  boot.initrd.kernelModules = [ ];
  boot.kernelModules = [ "kvm-amd" ];
  boot.extraModulePackages = [ ];
  fileSystems."/" =
    { device = "tank/root";
      fsType = "zfs";
    };
  fileSystems."/boot" =
    { device = "/dev/disk/by-partlabel/${hostName}.esp0";
      fsType = "vfat";
      options = [ "fmask=0022" "dmask=0022" ];
    };
  fileSystems."/boot1" =
    { device = "/dev/disk/by-partlabel/${hostName}.esp1";
      fsType = "vfat";
      options = [ "fmask=0022" "dmask=0022" ];
    };
  swapDevices = [ ];
  networking.useDHCP = lib.mkDefault true;
  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";
  hardware.cpu.amd.updateMicrocode = lib.mkDefault config.hardware.enableRedistributableFirmware;

  networking.hostName = hostName; # Define your hostname.
  # FIXME: breaks resolution of “ci0.servo.org” in libvirt guests
  # networking.domain = "servo.org";

  # Needed by ZFS.
  # Generate with: LC_ALL=C < /dev/urandom tr -dC 0-9A-F | head -c 8
  networking.hostId = hostId;

  # <https://docs.hetzner.com/robot/dedicated-server/network/net-config-cent-os/#dedicated-root-servers-1>
  networking.interfaces.eth0.ipv6.addresses = [ {
    address = ipv6Address;
    prefixLength = 64;
  } ];
  networking.defaultGateway6 = {
    address = "fe80::1";
    interface = "eth0";
  };

  # Pin nixpkgs flakeref to match our NixOS config, to avoid constantly fetching unstable packages.
  # <https://discourse.nixos.org/t/how-to-pin-nix-registry-nixpkgs-to-release-channel/14883/7>
  nix.registry.nixpkgs.to = { type = "path"; path = pkgs.path; };

  # Pin nixpkgs channel to nixpkgs flakeref.
  nix.nixPath = ["nixpkgs=flake:nixpkgs"];

  # When the NixOS configuration uses flakes, even old commands like nix-shell need flakes enabled.
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  # First version of NixOS ever installed with this config.
  system.stateVersion = "24.11";

  # Use GRUB instead of systemd-boot, so we can mirror the ESP across both disks.
  boot.loader.grub.mirroredBoots = [
    # One of them has to be /boot, which seems to be a GRUB or NixOS bug.
    # If we have /boot0 and /boot1, with an optional symlink from /boot to /boot0,
    # we generate a /boot0/grub/grub.cfg with “search --set=drive1 --label ci0”,
    # which makes no sense and does not work.
    { path = "/boot"; devices = ["/dev/disk/by-partlabel/${hostName}.esp0"]; }
    { path = "/boot1"; devices = ["/dev/disk/by-partlabel/${hostName}.esp1"]; }
  ];

  # Install for x86_64-efi platform (UEFI), not i386-pc (BIOS/CSM).
  boot.loader.grub.device = "nodev";
  boot.loader.grub.efiSupport = true;

  # Install to the removable boot path, to avoid relying on the NVRAM boot menu
  # which can get wiped or misconfigured.
  boot.loader.grub.efiInstallAsRemovable = true;

  # Don’t touch the NVRAM boot menu, in case we’re installing on a test machine.
  boot.loader.efi.canTouchEfiVariables = false;

  boot.kernelParams = [
    "default_hugepagesz=1G" "hugepagesz=1G" "hugepages=${toString hugepages}"

    # For benchmarking: isolate half of the CPUs from all processes and scheduling interrupts,
    # other than threads assigned by libvirt <vcpupin>.
    # <https://wiki.archlinux.org/index.php?title=PCI_passthrough_via_OVMF&oldid=845768#With_isolcpus_kernel_parameter>
    # <https://www.kernel.org/doc/html/latest/timers/no_hz.html>
    (lib.mkIf isBenchmarkingMachine "isolcpus=4-7,12-15")
    (lib.mkIf isBenchmarkingMachine "nohz_full=4-7,12-15")
  ];

  environment.systemPackages = with pkgs; [
    cdrkit  # for genisoimage
    clang
    dmg2img
    gh
    git
    hivex
    htop
    jq
    libguestfs-with-appliance  # for guestfish <https://github.com/NixOS/nixpkgs/issues/37540>
    ntfs3g
    python3
    ripgrep
    rustup
    sqlite
    tmux
    unzip
    uv
    vim
    virt-manager
    zsh
  ];

  services.openssh = {
    enable = true;
    settings.KbdInteractiveAuthentication = false;
    settings.PasswordAuthentication = false;
  };
  programs.mosh.enable = true;

  # Keep this in sync with ubuntu2204/user-data
  users.users.root.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGEpS5yFUgXwOf9rkw/TdZgoWkfAgLYwABGiK7qAWsHR root@ci0"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICBvkS7z2RAWzqRByRsHHB8PoCjXrnyHtjpdTxmOdcom delan@azabani.com/2016-07-18/Ed25519"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPag2UMaWyIEIsL0EbdvChBCcARVxNeJAplUZe70kXlr mrobinson@igalia.com"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIPiTpwVkwvE8npI7Z944tmrFlPKDjDVbm1rI7m4lng7 me@mukilan"
  ];

  virtualisation.libvirtd = {
    enable = true;
    qemu.runAsRoot = false;
    onShutdown = "shutdown";
  };

  services.fail2ban = {
    enable = true;
    ignoreIP = ["144.6.0.0/16" "2403:5808::/29"];
  };

  security.acme = {
    acceptTerms = true;
    certs."${hostName}.servo.org" = {
      email = "dazabani@igalia.com";
      webroot = "/var/lib/acme/acme-challenge";
      extraDomainNames = lib.mkIf hasIntermittentTracker [
        "intermittent-tracker.servo.org"
        "staging.intermittent-tracker.servo.org"
      ];
    };
  };
  users.users.nginx.extraGroups = [ "acme" ];
  services.nginx = {
    enable = true;
    # logError = "stderr notice";
    recommendedProxySettings = true;
    virtualHosts = let
      proxy = {
        extraConfig = ''
            # https://github.com/curl/curl/issues/674
            # https://trac.nginx.org/nginx/ticket/915
            proxy_hide_header Upgrade;
        '';
      };
      ssl = {
        useACMEHost = "${hostName}.servo.org";
        forceSSL = true;
      };
    in {
      "\"\"" = {
        locations."/" = proxy // {
          proxyPass = "http://[::1]:8000";
        };
        locations."/static/" = {
          extraConfig = ''
            # alias strips /static/ from url, unlike root
            # trailing slash avoids [error] opendir() ".../stati" failed
            alias ${pkgs.copyPathToStore ../../static}/;
          '';
        };
        locations."/chunker/" = {
          proxyPass = "http://[::1]:8001/";
        };
      } // ssl;
      "intermittent-tracker.servo.org" = lib.mkIf hasIntermittentTracker ({
        locations."/" = proxy // {
          proxyPass = "http://127.0.0.1:5000";
        };
      } // ssl);
      "staging.intermittent-tracker.servo.org" = lib.mkIf hasIntermittentTracker ({
        locations."/" = proxy // {
          proxyPass = "http://127.0.0.1:5001";
        };
      } // ssl);
    };
  };

  systemd.services = let
    perf-analysis-tools = pkgs.fetchFromGitHub {
      owner = "servo";
      repo = "perf-analysis-tools";
      rev = "d55694051ba381cc96f7f8fb2a4d6ebc03db947f";
      hash = "sha256-mROaUYqcOa+XePl4CzM/zM/mE21ejM2UhyQSYc8emc4=";
    };

    intermittent-tracker-service = workingDir: script: {
      # Wait for networking
      wants = ["network-online.target"];
      after = ["network-online.target"];

      # Start on boot.
      wantedBy = ["multi-user.target"];

      path = ["/run/current-system/sw"];
      inherit script;

      serviceConfig = {
        WorkingDirectory = workingDir;
        Restart = "on-failure";
      };
    };
    # v1 service requires manual deployment of the app:
    # $ git clone https://github.com/servo/intermittent-tracker.git <staging|prod>
    # $ cd <staging|prod>
    # $ uv venv
    # $ . .venv/bin/activate
    # $ uv pip install -r requirements.txt
    # $ cp config.json.example config.json
    intermittent-tracker-service-v1 = workingDir:
      intermittent-tracker-service workingDir ''
        . .venv/bin/activate
        FLASK_DEBUG=1 python3 -m intermittent_tracker.flask_server
      '';
    intermittent-tracker-service-v2 = workingDir: app:
      intermittent-tracker-service workingDir ''
        FLASK_DEBUG=1 ${app}/bin/intermittent_tracker
      '';

    intermittent-tracker-staging = pkgs.callPackage ./python-app.nix {
      src = pkgs.fetchFromGitHub {
        owner = "servo";
        repo = "intermittent-tracker";
        rev = "42d55fdcce5e1d4e3aec70e4cfeb575d8569c8d3";
        hash = "sha256-L8Sk1aL3Na0JFymFj07eCcFIUZ0uYLt1ED2DDKnZ4VU=";
      };
      packageName = "intermittent-tracker";
    };
  in {
    # For benchmarking: disable CPU frequency boost, offline SMT siblings, etc.
    # Process affinity will be isolated externally via `isolcpus`.
    isolate-cpu = lib.mkIf isBenchmarkingMachine {
      # Start on boot.
      wantedBy = ["multi-user.target"];

      # Block libvirtd until started.
      before = ["libvirtd.service"];

      path = ["/run/current-system/sw"];
      script = ''
        ${perf-analysis-tools}/isolate-cpu-for-hypervisor.sh 4 5 6 7
      '';

      serviceConfig = {
        Type = "oneshot";
      };
    };

    intermittent-tracker-staging = lib.mkIf hasIntermittentTracker
      (intermittent-tracker-service-v2
        "/config/intermittent-tracker/staging"
        intermittent-tracker-staging);
    intermittent-tracker-prod = lib.mkIf hasIntermittentTracker
      (intermittent-tracker-service-v1
        "/config/intermittent-tracker/prod");

    monitor = {
      # Wait for networking
      wants = ["network-online.target"];
      after = ["network-online.target"];

      # Start on boot.
      wantedBy = ["multi-user.target"];

      script = ''
        RUST_LOG=monitor=info,cmd_lib::child=info ${monitor}/bin/monitor
      '';

      serviceConfig = {
        WorkingDirectory = "/config/monitor";
        Restart = "on-failure";
      };
    };

    chunker = {
      # Wait for networking
      wants = ["network-online.target"];
      after = ["network-online.target"];

      # Start on boot.
      wantedBy = ["multi-user.target"];

      script = ''
        ${monitor}/bin/chunker
      '';

      serviceConfig = {
        WorkingDirectory = "/config/monitor";
        Restart = "on-failure";
      };
    };
  };

  networking.firewall.allowedTCPPorts = [
    80 443  # nginx
    8000  # monitor (for guests)
  ];
}
