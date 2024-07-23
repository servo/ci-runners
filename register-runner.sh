#!/usr/bin/env zsh
# usage: register-runner.sh <work_folder> <platform_label> <vm>
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
work_folder=$1; shift
platform_label=$1; shift
vm=$1; shift
export LIBVIRT_DEFAULT_URI=qemu:///system

gh api --method POST -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" \
    /repos/delan/servo/actions/runners/generate-jitconfig \
    -f "name=$vm" -F "runner_group_id=1" -f 'work_folder='"$work_folder" \
    -f "labels[]=self-hosted" -f "labels[]=X64" -f "labels[]=$platform_label"
