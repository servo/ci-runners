#!/usr/bin/env zsh
# usage: configure-runner.sh <vm>
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
cache_dir=$script_dir/cache
vm=$1; shift
. "$script_dir/inject.sh"

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject a/servo /mnt/servo0/servo

>&2 echo '[*] Injecting cargo cache'
inject Users/Administrator /mnt/servo0/.cargo

>&2 echo '[*] Injecting prejob.ps1'
inject init "$script_dir/prejob.ps1"

>&2 echo '[*] Injecting job.ps1'
inject init "$script_dir/job.ps1"
