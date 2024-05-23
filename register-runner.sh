#!/usr/bin/env zsh
# usage: register-runner.sh <vm>
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
vm=$1; shift
export LIBVIRT_DEFAULT_URI=qemu:///system

gh api --method POST -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" \
    /repos/delan/servo/actions/runners/generate-jitconfig \
    -f "name=$vm" -F "runner_group_id=1" -f 'work_folder=..\a' \
    -f "labels[]=self-hosted" -f "labels[]=X64" -f "labels[]=Windows" \
    -q .encoded_jit_config
