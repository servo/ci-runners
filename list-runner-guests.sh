#!/usr/bin/env zsh
# usage: list-runner-vms.sh
script_dir=${0:a:h}
. "$script_dir/common.sh"

virsh list --name
