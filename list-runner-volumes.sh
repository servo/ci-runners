#!/usr/bin/env zsh
# usage: list-runner-vms.sh
script_dir=${0:a:h}
. "$script_dir/common.sh"

zfs list -Ho name -rt volume $SERVO_CI_ZFS_PREFIX
