#!/usr/bin/env zsh
# usage: mount-runner.sh <vm> [command]
set -euo pipefail -o bsdecho
vm=$1; shift
command=${1-zsh}
export LIBVIRT_DEFAULT_URI=qemu:///system

mount=$(mktemp -d)
mount /dev/zvol/cuffs/$vm-part2 $mount
( cd $mount; nix-shell -p hivex --run "$command $vm" || : )
umount $mount
