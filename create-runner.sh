#!/usr/bin/env zsh
# usage: create-runner.sh <base_vm> <base_snapshot> <path/to/configure-runner.sh> <runner_jitconfig_cmd [args ...]>
# runner_jitconfig_cmd should be a command like `$PWD/register-runner.sh ../a Linux`
script_dir=${0:a:h}
. "$script_dir/common.sh"
base_vm=$1; shift
base_snapshot=$base_vm@$1; shift
configure_runner=$1; shift

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
>&2 echo "Runner id is $(jq .runner.id $runner_jitconfig)"
"$script_dir/mount-runner.sh" $vm "$configure_runner '$(jq .encoded_jit_config $runner_jitconfig)'"

virt-clone --preserve-data --check path_in_use=off -o $base_vm -n $vm -f /dev/zvol/cuffs/$vm
virsh start $vm

printf 'Ready to destroy? '
read -r
./destroy-runner.sh $base_vm $i
