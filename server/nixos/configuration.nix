# Edit this configuration file to define what should be installed on
# your system. Help is available in the configuration.nix(5) man page, on
# https://search.nixos.org/options and in the NixOS manual (`nixos-help`).

{ config, lib, pkgs, ... }:

{
  imports =
    [ # Include the results of the hardware scan.
      ./hardware-configuration.nix
    ];

  networking.hostName = "ci0"; # Define your hostname.
  networking.domain = "servo.org";

  # Needed by ZFS.
  # Generate with: LC_ALL=C < /dev/urandom tr -dC 0-9A-F | head -c 8
  networking.hostId = "04AA04E2";

  # First version of NixOS ever installed with this config.
  system.stateVersion = "24.11";

  # Use GRUB instead of systemd-boot, so we can mirror the ESP across both disks.
  boot.loader.grub.mirroredBoots = [
    # One of them has to be /boot, which seems to be a GRUB or NixOS bug.
    # If we have /boot0 and /boot1, with an optional symlink from /boot to /boot0,
    # we generate a /boot0/grub/grub.cfg with “search --set=drive1 --label ci0”,
    # which makes no sense and does not work.
    { path = "/boot"; devices = ["/dev/disk/by-partlabel/ci0.esp0"]; }
    { path = "/boot1"; devices = ["/dev/disk/by-partlabel/ci0.esp1"]; }
  ];

  # Install for x86_64-efi platform (UEFI), not i386-pc (BIOS/CSM).
  boot.loader.grub.device = "nodev";
  boot.loader.grub.efiSupport = true;

  # Install to the removable boot path, to avoid relying on the NVRAM boot menu
  # which can get wiped or misconfigured.
  boot.loader.grub.efiInstallAsRemovable = true;

  # Don’t touch the NVRAM boot menu, in case we’re installing on a test machine.
  boot.loader.efi.canTouchEfiVariables = false;

  environment.systemPackages = with pkgs; [
    gh
    git
    hivex
    jq
    rustup
    unzip
    vim
    zsh
  ];

  services.openssh.enable = true;
  users.users.root.openssh.authorizedKeys.keys = ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICBvkS7z2RAWzqRByRsHHB8PoCjXrnyHtjpdTxmOdcom delan@azabani.com/2016-07-18/Ed25519"];
}
