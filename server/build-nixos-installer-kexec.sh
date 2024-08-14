#!/usr/bin/env zsh
set -xeuo pipefail -o bsdecho

# https://nixos.wiki/wiki/Install_NixOS_on_Hetzner_Online#Bootstrap_from_the_Rescue_System
config_file=$(mktemp)
cat <<'EOF' > "$config_file"
{
  services.openssh.enable = true;
  users.users.root.openssh.authorizedKeys.keys = ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICBvkS7z2RAWzqRByRsHHB8PoCjXrnyHtjpdTxmOdcom delan@azabani.com/2016-07-18/Ed25519"];
}
EOF
nix run github:nix-community/nixos-generators/a220fc3a6e144f12f0c3dc3e4d01d44c2e6b0b85 -- -o result -f kexec-bundle -c "$config_file"
