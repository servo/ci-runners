#!/usr/bin/env zsh
# usage: mount-runner.sh <vm> [command]
script_dir=${0:a:h}
. "$script_dir/common.sh"
vm=$1; shift
command=${1-zsh}

mount=$(mktemp -d)
mount /dev/zvol/cuffs/$vm-part2 $mount
( cd $mount; nix-shell -p hivex unzip --run "$command $vm" || : )
umount $mount
