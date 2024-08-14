#!/usr/bin/env bash
set -xeuo pipefail

# https://nixos.wiki/wiki/Install_NixOS_on_Hetzner_Online#Bootstrap_from_the_Rescue_System
apt install -y sudo
mkdir -p /etc/nix
echo "build-users-group =" > /etc/nix/nix.conf
curl -L https://nixos.org/nix/install | sh
. $HOME/.nix-profile/etc/profile.d/nix.sh
nix-env -f https://github.com/nix-community/nixos-generators/archive/1.7.0.tar.gz -i -v
cat <<'EOF' > /root/config.nix
{
  services.openssh.enable = true;
  users.users.root.openssh.authorizedKeys.keys = ["ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICBvkS7z2RAWzqRByRsHHB8PoCjXrnyHtjpdTxmOdcom delan@azabani.com/2016-07-18/Ed25519"];
}
EOF
nixos-generate -o /root/result -f kexec-bundle -c /root/config.nix
/root/result
