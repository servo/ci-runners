#!/usr/bin/env zsh
# usage: destroy-runner.sh <base_vm> <runner_number>
script_dir=${0:a:h}
. "$script_dir/common.sh"
base_vm=$1; shift
vm=$base_vm.$1; shift
libvirt_vm=$SERVO_CI_LIBVIRT_PREFIX-$vm

virsh destroy $libvirt_vm || :
virsh undefine --nvram $libvirt_vm || :
zfs destroy -v $SERVO_CI_ZFS_PREFIX/$vm
