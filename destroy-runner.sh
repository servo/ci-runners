#!/usr/bin/env zsh
# usage: destroy-runner.sh <base_vm> <runner_number>
set -euo pipefail -o bsdecho
base_vm=$1; shift
vm=$base_vm.$1; shift
export LIBVIRT_DEFAULT_URI=qemu:///system

virsh destroy $vm || :
virsh undefine $vm
zfs destroy -v cuffs/$vm
