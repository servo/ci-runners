#!/usr/bin/env zsh
# usage: update.sh
# requires: git nixos-rebuild
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
nixos_dir=$script_dir/nixos

cd "$script_dir"
git pull
rm -Rf /etc/nixos
ln -sr "$nixos_dir" /etc/nixos
nixos-rebuild switch
