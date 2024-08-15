#!/usr/bin/env zsh
# usage: update.sh <path/to/nixos>
# requires: git nixos-rebuild
if [ $# -lt 1 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
nixos_dir=${1:a}

cd "$script_dir"
git pull
cp -R "$nixos_dir" /etc
nixos-rebuild switch
