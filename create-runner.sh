#!/usr/bin/env zsh
# usage: create-runner.sh <id> <base_vm> <base_snapshot> <configuration_name>
script_dir=${0:a:h}
. "$script_dir/common.sh"
id=$1; shift
base_vm=$1; shift
base_snapshot=$base_vm@$1; shift
configuration_name=$1; shift
configure_runner=$script_dir/$configuration_name/configure-runner.sh
register_runner=$script_dir/$configuration_name/register-runner.sh

vm=$base_vm.$id
>&2 printf '[*] Creating runner: %s\n' $vm

runner_data=$SERVO_CI_MONITOR_DATA_PATH/$id
mkdir $runner_data
touch $runner_data/created-time

zfs clone $SERVO_CI_ZFS_CLONE_PREFIX/$base_snapshot $SERVO_CI_ZFS_PREFIX/$vm
while ! test -e /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm-part2; do
    sleep 1
done

if ! [ -n "${SERVO_CI_DONT_REGISTER_RUNNERS+set}" ]; then
    $register_runner $vm > $runner_data/github-api-registration
    >&2 echo "GitHub API runner id is $(jq .runner.id $runner_data/github-api-registration)"
    runner_jitconfig=$(jq .encoded_jit_config $runner_data/github-api-registration)
else
    >&2 echo 'Skipping GitHub API registration (SERVO_CI_DONT_REGISTER_RUNNERS)'
    runner_jitconfig=
fi
"$script_dir/mount-runner.sh" $vm $configure_runner "$runner_jitconfig"

libvirt_vm=$SERVO_CI_LIBVIRT_PREFIX-$vm
virt-clone --preserve-data --check path_in_use=off -o $base_vm -n $libvirt_vm -f /dev/zvol/$SERVO_CI_ZFS_PREFIX/$vm
virsh start $libvirt_vm
