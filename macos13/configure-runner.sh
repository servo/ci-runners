#!/usr/bin/env zsh
# usage: configure-runner.sh <runner_jitconfig> <vm>
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
cache_dir=$script_dir/cache
runner_jitconfig=$1; shift
vm=$1; shift

>&2 echo '[*] Injecting GitHub Actions config'
# Register the runner as both ephemeral (one job only) and just-in-time (no token required)
# See also: <https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions#using-just-in-time-runners>
> init/runner.sh echo 'mkdir -p ~/actions-runner'
>> init/runner.sh echo 'cd ~/actions-runner'
>> init/runner.sh echo 'tar xf /Volumes/a/init/actions-runner-osx-x64.tar.gz'
>> init/runner.sh echo '~/actions-runner/run.sh --jitconfig '"$runner_jitconfig"
