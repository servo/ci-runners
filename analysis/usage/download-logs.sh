#!/bin/sh
# Usage: download-logs.sh <host ...> [-- extra journalctl options]
#   e.g. download-logs.sh ci{0..4}.servo.org
set -eu
double_dash=false
hosts=
# Remove hosts and double dash if any
for arg in "$@"; do
    shift
    if [ "$arg" = -- ]; then
        double_dash=true
        break
    else
        hosts=$hosts${hosts:+ }$arg
    fi
done
# Escape the extra journalctl options
for arg in "$@"; do
    shift
    set -- "$@" "$(printf \%s "$arg" | sed 's/./\\&/g')"
done
set -x

for host in $hosts; do
    ssh "root@$host" journalctl -u monitor "$@" \| dd bs=1M status=progress \| zstd > "$host.log.zst"
    unzstd "$host.log.zst"
done
