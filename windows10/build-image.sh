#!/usr/bin/env zsh
# usage: windows10/build-image.sh
image_dir=${0:a:h}
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
trap print_undo_commands EXIT
cache_dir=$script_dir/cache
. "$script_dir/download.sh"
. "$script_dir/inject.sh"
undo_commands=$(mktemp)
image_name=servo-windows10

>&2 echo '[*] Caching downloads'
download "$cache_dir" https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.240-1/virtio-win-0.1.240.iso ebd48258668f7f78e026ed276c28a9d19d83e020ffa080ad69910dc86bbcbcc6

>&2 echo '[*] Creating zvol and libvirt guest'
zfs create -V 90G "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new"
>> $undo_commands echo "zfs destroy '$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new'"
virsh define -- "$image_dir/guest.xml"
>> $undo_commands echo "virsh undefine -- '$image_name.init'"
virt-clone --preserve-data --check path_in_use=off -o "$image_name.init" -n "$image_name.new" -f "/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new"
>> $undo_commands echo "virsh undefine -- '$image_name.new'"
virsh undefine -- "$image_name.init"

>&2 echo '[*] Writing disk images'
inject /var/lib/libvirt/images "$cache_dir/virtio-win-0.1.240.iso"
genisoimage -J -o "/var/lib/libvirt/images/$image_name.config.iso" "$image_dir/autounattend.xml"

>&2 echo '[*] Starting guest, to install Windows'
virsh start "$image_name.new"

>&2 echo '[*] Waiting for guest to shut down (max 640 seconds)'  # normally ~313 seconds
if ! time virsh event --timeout 640 -- "$image_name.new" lifecycle; then
    >&2 echo 'virsh event timed out!'
    exit 1
fi

>&2 echo '[*] Waiting for partition block device to appear'
partition_block_device=/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new-part1
t=0; while ! test -e $partition_block_device; do
    if [ $t -ge $SERVO_CI_ZVOL_BLOCK_DEVICE_TIMEOUT ]; then
        >&2 printf '[!] Timed out waiting for block device: %s' $partition_block_device
        exit 1
    fi
    sleep 1
    t=$((t+1))
done

>&2 echo '[*] Forcing update of partition block device geometry'
# Dec 20 17:12:59 jupiter kernel: EXT4-fs (zd16p1): bad geometry: block count 23564539 exceeds size of device (548091 blocks)
blockdev --rereadpt "/dev/zvol/$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new"

>&2 echo '[*] Configuring base image'
./mount-runner.sh "$image_name.new" "$image_dir/configure-base.sh"

>&2 echo '[*] Starting guest, to apply changes'
virsh start "$image_name.new"

>&2 echo '[*] Waiting for guest to shut down (max 2500 seconds)'  # normally ~1218 seconds
if ! time virsh event --timeout 2500 -- "$image_name.new" lifecycle; then
    >&2 echo 'virsh event timed out!'
    exit 1
fi

>&2 echo "[*] Taking zvol snapshot: $SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new@ready"
zfs snapshot "$SERVO_CI_ZFS_CLONE_PREFIX/$image_name.new@ready"

# TODO: check that servo was actually built correctly

>&2 echo '[*] Done!'
