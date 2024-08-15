#!/usr/bin/env zsh
# usage: install-or-reinstall.sh <path/to/nixos> <path/to/mnt>
# requires: nixos-install
if [ $# -lt 2 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
nixos_dir=$1
mnt_dir=$2

mkdir -p "$mnt_dir/etc"
cp -R "$nixos_dir" "$mnt_dir/etc"
nixos-install --no-root-password --root "$mnt_dir"
