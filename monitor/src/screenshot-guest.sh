#!/usr/bin/env zsh
# usage: screenshot-guest.sh <libvirt guest name> <output path>
set -euo pipefail -o bsdecho
guest_name=$1; shift
output_path=$1; shift

virsh screenshot -- "$guest_name" "$output_path" > /dev/null
