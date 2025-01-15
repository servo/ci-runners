#!/usr/bin/env zsh
set -xeuo pipefail -o bsdecho

# https://nixos.wiki/wiki/Install_NixOS_on_Hetzner_Online#Bootstrap_from_the_Rescue_System
config_file=$(mktemp)
cat <<'EOF' > "$config_file"
{ pkgs, ... }:
{
  services.openssh.enable = true;
  users.users.root.openssh.authorizedKeys.keys = ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICBvkS7z2RAWzqRByRsHHB8PoCjXrnyHtjpdTxmOdcom delan@azabani.com/2016-07-18/Ed25519"];
  environment.systemPackages = with pkgs; [ git zsh jq ];
}
EOF
nix run github:nix-community/nixos-generators/74b8e31dd709760c86eed16b6c1d0b88d7360937 -- -o result -f kexec-bundle -c "$config_file"
