#!/usr/bin/env zsh
# usage: register-runner.sh <github_runner_label> <vm>
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
github_runner_label=$1; shift
vm=$1; shift
$script_dir/register-runner.sh ../a "$github_runner_label" $vm
