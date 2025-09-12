#!/usr/bin/env zsh
# usage: first-time-install.sh <hostname> <disk> [disk ...]
# requires: sfdisk jq fgrep umount mount zpool zfs mkfs.vfat
if [ $# -lt 2 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
esp_size_MiB=1024
swap_size_MiB=1024
hostname=$1; shift

# To test locally:
# - truncate -s 20G disk0
# - truncate -s 20G disk1
# - sudo losetup -Pf --show disk0
# - sudo losetup -Pf --show disk1
# - sudo ./install-nixos.sh ci0 /dev/loopX /dev/loopY

i=0
for disk_dev; do
    # Check if the disk has any partitions in use.
    sfdisk -J "$disk_dev" | jq -er '.partitiontable.partitions[].node' | while read partition_dev; do
        # If the partition is mounted, unmount it.
        if fgrep -q "${partition_dev} " /etc/mtab; then
            umount "$partition_dev"
        fi
        # If the disk has an active zpool, list it and exit.
        zpool get -Ho value name | while read zpool_name; do
            if zpool list -vHLP "$zpool_name" | fgrep -q $'\t'"$partition_dev"$'\t'; then
                zpool list -vHLP "$zpool_name"
                set +x
                >&2 echo "fatal: disk has active zpool"
                >&2 echo 'if you are retrying the install because it failed for nix reasons, consider rerunning that part only:'
                >&2 echo "$ ./install-or-reinstall.sh $hostname /mnt/$hostname"
                exit 1
            fi
        done
    done

    printf \%s\\n \
        'label: gpt' \
        "size=${esp_size_MiB}MiB, type=uefi, name=${hostname}.esp$i" \
        "size=${swap_size_MiB}MiB, type=swap, name=${hostname}.swap$i" \
        "type=linux, name=${hostname}.tank$i" \
    | sfdisk "$disk_dev"
    sfdisk -J "$disk_dev" | jq -er '.partitiontable.partitions[0].node' | read esp_dev
    sfdisk -J "$disk_dev" | jq -er '.partitiontable.partitions[1].node' | read swap_dev
    sfdisk -J "$disk_dev" | jq -er '.partitiontable.partitions[2].node' | read tank_dev

    # If you run mkfs.fat in a nix-shell, the default volume id will be 12CE-A600 every time,
    # which interacts poorly with things that rely on fs uuids like GRUB’s “search --uuid”.
    # This is because nix-shell sets SOURCE_DATE_EPOCH and mkfs.fat uses that verbatim.
    ( unset SOURCE_DATE_EPOCH; mkfs.fat -F 32 "$esp_dev" )

    i=$((i+1))
done

zpool create -t "$hostname" -o ashift=12 -O mountpoint=none -O acltype=posixacl -O xattr=sa -R "/mnt/$hostname" tank mirror "/dev/disk/by-partlabel/${hostname}.tank"[0-9]*
zfs create -o mountpoint=legacy "$hostname/root"
mkdir -p "/mnt/$hostname"
mount -t zfs "$hostname/root" "/mnt/$hostname"

i=0
while [ $i -lt $# ]; do
    if [ $i -gt 0 ]; then
        boot_dir=/mnt/$hostname/boot$i
    else
        boot_dir=/mnt/$hostname/boot
    fi
    mkdir -p "$boot_dir"
    mount "/dev/disk/by-partlabel/${hostname}.esp$i" "$boot_dir"
    i=$((i+1))
done

./install-or-reinstall.sh "$hostname" "/mnt/$hostname"
