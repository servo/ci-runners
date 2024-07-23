#!/usr/bin/env zsh
# usage: configure-runner.sh <runner_jitconfig> <vm>
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
cache_dir=$script_dir/cache
runner_jitconfig=$1; shift
vm=$1; shift

>&2 echo '[*] Injecting GitHub Actions config'
# FIXME: “Must not run interactively with sudo” in /actions-runner/run-helper.sh
> init/runner.sh echo 'export RUNNER_ALLOW_RUNASROOT=1'
# Register the runner as both ephemeral (one job only) and just-in-time (no token required)
# See also: <https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions#using-just-in-time-runners>
>> init/runner.sh echo '/actions-runner/run.sh --jitconfig '"$runner_jitconfig"
