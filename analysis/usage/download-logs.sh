#!/bin/sh
# Usage: download-logs.sh <host ...>
#   e.g. download-logs.sh ci{0..4}.servo.org
for host in "$@"; do
    ssh "root@$host" 'journalctl -u monitor | dd bs=1M status=progress | zstd' > "$host.log.zst"
    unzstd "$host.log.zst"
done
