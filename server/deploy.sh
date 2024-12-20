#!/usr/bin/env zsh
# usage: deploy.sh
# requires: nixos-rebuild
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
nixos_dir=$script_dir/nixos

cd "$script_dir"
( cd ../monitor && cargo build )
rm -Rf /etc/nixos
ln -sr "$nixos_dir" /etc/nixos
nixos-rebuild switch
