#!/usr/bin/env zsh
# usage: get-snapshot-creation.sh <pool/path/to/zvol@snapshot>
set -euo pipefail -o bsdecho
dataset_and_snapshot=$1; shift

zfs get -Hpo value creation "$dataset_and_snapshot"
