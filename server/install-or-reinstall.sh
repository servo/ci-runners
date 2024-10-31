#!/usr/bin/env zsh
# usage: install-or-reinstall.sh <hostname> <path/to/mnt>
# requires: nixos-install
if [ $# -lt 2 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
nixos_dir=$script_dir/nixos
mnt_dir=$1
hostname=$2

mkdir -p "$mnt_dir/etc"
cp -R "$nixos_dir" "$mnt_dir/etc"

# Like `nixos-install --flake .\#$hostname`, but avoids the error in NixOS/nix#4081:
# <https://github.com/NixOS/nix/issues/4081#issuecomment-753237142>
cd "$nixos_dir"
nix build .\#nixosConfigurations."$hostname".config.system.build.toplevel
# `--no-root-password` means keep the root password from configuration.nix.
nixos-install --no-root-password --root / --system ./result
rm result
