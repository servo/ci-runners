#!/usr/bin/env zsh
# usage: macos13/build-image.sh <snapshot_name>
image_dir=${0:a:h}
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
trap print_undo_commands EXIT
cache_dir=$script_dir/cache
. "$script_dir/download.sh"
. "$script_dir/inject.sh"
undo_commands=$(mktemp)
image_name=servo-macos13
snapshot_name=$1; shift

>&2 echo '[*] Caching downloads'
download "$cache_dir" https://cloud-images.ubuntu.com/jammy/20241217/jammy-server-cloudimg-amd64.img 0d8345a343c2547e55ac815342e6cb4a593aa5556872651eb47e6856a2bb0cdd

>&2 echo '[*] Creating zvol (if needed)'
# TODO: find a more efficient way to do an idempotent zfs-clone(8) that retains the clone’s old snapshots?
zfs list -Ho name "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name" || zfs create -V 90G "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"

>&2 echo '[*] Creating libvirt guest (or recreating it with new config)'
if virsh domstate -- "$image_name"; then
    virsh destroy -- "$image_name" || :  # FIXME make this idempotent in a less noisy way?
    virsh undefine --nvram -- "$image_name"
fi
virt-clone --preserve-data --check path_in_use=off -o "$image_name.clean" -n "$image_name" --nvram /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.$image_name.fd --skip-copy sda -f /dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name --skip-copy sdc
cp /var/lib/libvirt/images/OSX-KVM/OVMF_VARS.{$image_name.clean,$image_name}.fd

>&2 echo '[*] Writing disk image'
# TODO: find a more efficient way to do an idempotent zfs-clone(8) that retains the clone’s old snapshots?
dd status=progress bs=1M if="/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.clean@automated" of="/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"

>&2 echo '[*] Forcing update of partition block device geometry'
# Dec 20 17:12:59 jupiter kernel: EXT4-fs (zd16p1): bad geometry: block count 23564539 exceeds size of device (548091 blocks)
blockdev --rereadpt "/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"
sleep 1

>&2 echo '[*] Waiting for partition block device to appear'
partition_block_device=/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name-part1
t=0; while ! test -e $partition_block_device; do
    if [ $t -ge $SERVO_CI_ZVOL_BLOCK_DEVICE_TIMEOUT ]; then
        >&2 printf '[!] Timed out waiting for block device: %s' $partition_block_device
        exit 1
    fi
    sleep 1
    t=$((t+1))
done

>&2 echo '[*] Configuring base image'
./mount-runner.sh "$image_name" "$image_dir/configure-base.sh"

>&2 echo '[*] Starting guest, to apply changes'
virsh start "$image_name"

>&2 echo '[*] Waiting for guest to shut down (max 2000 seconds)'  # normally ~850 seconds
if ! time virsh event --timeout 2000 -- "$image_name" lifecycle; then
    >&2 echo 'virsh event timed out!'
    exit 1
fi

>&2 echo '[*] Checking that Servo was built correctly'
./mount-runner.sh "$image_name" sh -c 'ls init/built_servo_once_successfully'

>&2 echo "[*] Taking zvol snapshot: $SERVO_CI_ZFS_CLONE_PREFIX/$image_name@$snapshot_name"
zfs snapshot "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name@$snapshot_name"

>&2 echo '[*] Done!'
