#!/usr/bin/env zsh
# usage: virt-clone.sh <base_vm> <vm>
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
base_vm=$1; shift
vm=$1; shift

libvirt_vm=$SERVO_CI_LIBVIRT_PREFIX-$vm
virt-clone --preserve-data --check path_in_use=off -o $base_vm -n $libvirt_vm -f /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm
