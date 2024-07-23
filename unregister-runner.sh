#!/usr/bin/env zsh
# usage: unregister-runner.sh <id>
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
id=$1; shift
export LIBVIRT_DEFAULT_URI=qemu:///system

gh api --method DELETE -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" \
    /repos/delan/servo/actions/runners/$id
