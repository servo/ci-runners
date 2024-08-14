#!/usr/bin/env zsh
# usage: install-nixos.sh <hostname> <disk> [disk ...]
# requires: sfdisk jq fgrep umount mount zpool zfs mkfs.vfat nixos-install
if [ $# -lt 1 ]; then >&2 sed '2!d;2s/^# //;2q' "$0"; exit 1; fi
set -xeuo pipefail -o bsdecho
esp_size_MiB=1024
swap_size_MiB=1024
hostname=$1; shift

# To test locally:
# - truncate -s 20G 0
# - truncate -s 20G 1
# - sudo losetup -Pf --show 0
# - sudo losetup -Pf --show 1
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
                >&2 echo "fatal: disk has active zpool"
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
    mkfs.vfat -F 32 "$esp_dev"

    i=$((i+1))
done

zpool create -t "$hostname" -o ashift=12 -O mountpoint=none -O acltype=posixacl -O xattr=sa -R "/mnt/$hostname" tank mirror "/dev/disk/by-partlabel/${hostname}.tank"[0-9]*
zfs create -o mountpoint=legacy "$hostname/root"
mkdir -p "/mnt/$hostname"
mount -t zfs "$hostname/root" "/mnt/$hostname"
ln -s boot0 "/mnt/$hostname/boot"

i=0
while [ $i -lt $# ]; do
    mkdir "/mnt/$hostname/boot$i"
    mount "/dev/disk/by-partlabel/${hostname}.esp$i" "/mnt/$hostname/boot$i"
    i=$((i+1))
done

mkdir "/mnt/$hostname/etc"
cp -R nixos "/mnt/$hostname/etc"
nixos-install --no-root-password --root "/mnt/$hostname"
