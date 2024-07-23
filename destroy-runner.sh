#!/usr/bin/env zsh
# usage: destroy-runner.sh <base_vm> <runner_number>
script_dir=${0:a:h}
. "$script_dir/common.sh"
base_vm=$1; shift
vm=$base_vm.$1; shift

virsh destroy $vm || :
virsh undefine $vm
zfs destroy -v cuffs/$vm
