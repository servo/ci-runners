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
if [ -e /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm-part1 ]; then
    mount /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm-part1 $mount
elif [ -e /dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$vm-part1 ]; then
    mount /dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$vm-part1 $mount
else
    >&2 echo "fatal: failed to find $vm-part1 in /dev/zvol/$SERVO_CI_ZFS_PREFIX or /dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX"
    exit 1
fi
( cd $mount; "$@" || : )
umount $mount
