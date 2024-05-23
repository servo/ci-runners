#!/usr/bin/env zsh
# usage: create-runner.sh <base_vm> <base_snapshot> <runner_jitconfig_cmd>
# runner_jitconfig_cmd should be a command like `sudo -iu delan $PWD/register-runner.sh`
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
base_vm=$1; shift
base_snapshot=$base_vm@$1; shift
export LIBVIRT_DEFAULT_URI=qemu:///system

i=0; while zfs list -Ho volsize cuffs/$base_vm.$i > /dev/null 2>&1; do
    i=$((i+1))
done
vm=$base_vm.$i
>&2 printf '[*] Creating runner: %s\n' $vm

zfs clone cuffs/{$base_snapshot,$vm}
while ! test -e /dev/zvol/cuffs/$vm-part2; do
    sleep 1
done

runner_jitconfig=$(mktemp)
> $runner_jitconfig "$@" $vm
"$script_dir/mount-runner.sh" $vm "$script_dir/configure-runner.sh '$(cat $runner_jitconfig)'"

virt-clone --preserve-data --check path_in_use=off -o $base_vm -n $vm -f /dev/zvol/cuffs/$vm
virsh start $vm

printf 'Ready to destroy? '
read -r
./destroy-runner.sh $base_vm $i
