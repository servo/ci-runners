#!/usr/bin/env zsh
# usage: mount-runner.sh <vm> [command [args ...]]
script_dir=${0:a:h}
. "$script_dir/common.sh"
vm=$1; shift
if [ $# -lt 1 ]; then
    set -- zsh
else
    set -- "$@" $vm
fi

mount=$(mktemp -d)
mount /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm-part2 $mount
( cd $mount; "$@" || : )
umount $mount
