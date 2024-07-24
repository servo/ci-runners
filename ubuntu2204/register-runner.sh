#!/usr/bin/env zsh
# usage: register-runner.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
vm=$1; shift
$script_dir/register-runner.sh ../a Linux $vm
