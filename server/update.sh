#!/usr/bin/env zsh
# usage: update.sh
# requires: git nixos-rebuild
if [ $# -lt 1 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
nixos_dir=$script_dir/nixos

cd "$script_dir"
git pull
cp -R "$nixos_dir" /etc
nixos-rebuild switch
