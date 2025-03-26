#!/usr/bin/env zsh
# usage: windows10/build-image.sh <snapshot_name>
image_dir=${0:a:h}
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
trap print_undo_commands EXIT
. "$script_dir/download.sh"
. "$script_dir/inject.sh"
undo_commands=$(mktemp)
image_name=servo-windows10
snapshot_name=$1; shift
cd -- "$script_dir"

>&2 echo '[*] Creating zvol (if needed)'
zfs list -Ho name "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name" || zfs create -V 90G "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"

>&2 echo '[*] Creating libvirt guest (or recreating it with new config)'
if virsh domstate -- "$image_name"; then
    virsh destroy -- "$image_name" || :  # FIXME make this idempotent in a less noisy way?
    virsh undefine -- "$image_name"
fi
virsh define -- "$image_dir/guest.xml"
virt-clone --preserve-data --check path_in_use=off -o "$image_name.init" -n "$image_name" -f "/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"
virsh undefine -- "$image_name.init"

>&2 echo '[*] Wiping zvol'
dd bs=1M count=1 if=/dev/zero of="/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name"

>&2 echo '[*] Writing disk images'
inject_regular_file /var/lib/libvirt/images "$IMAGE_DEPS_DIR/windows10/virtio-win-0.1.240.iso"
genisoimage -J -o "/var/lib/libvirt/images/$image_name.config.iso" "$image_dir/autounattend.xml"

>&2 echo '[*] Starting guest, to install Windows'
virsh start "$image_name"

>&2 echo '[*] Waiting for guest to shut down (max 640 seconds)'  # normally ~313 seconds
if ! time virsh event --timeout 640 -- "$image_name" lifecycle; then
    >&2 echo 'virsh event timed out!'
    exit 1
fi

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

>&2 echo '[*] Waiting for guest to shut down (max 2500 seconds)'  # normally ~1218 seconds
if ! time virsh event --timeout 2500 -- "$image_name" lifecycle; then
    >&2 echo 'virsh event timed out!'
    exit 1
fi

>&2 echo '[*] Checking that Servo was built correctly'
./mount-runner.sh "$image_name" sh -c 'ls -l init/built_servo_once_successfully'

>&2 echo "[*] Taking zvol snapshot: $SERVO_CI_ZFS_CLONE_PREFIX/$image_name@$snapshot_name"
zfs snapshot "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name@$snapshot_name"

>&2 echo '[*] Done!'
