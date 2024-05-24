#!/usr/bin/env zsh
# usage: configure-runner.sh <runner_jitconfig> <vm>
set -euo pipefail -o bsdecho
script_dir=${0:a:h}
cache_dir=$script_dir/cache
runner_jitconfig=$1; shift
vm=$1; shift

>&2 echo '[*] Injecting GitHub Actions config'
> init/runner.ps1 echo '. C:\init\refreshenv.ps1'
# Register the runner as both ephemeral (one job only) and just-in-time (no token required)
# See also: <https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions#using-just-in-time-runners>
>> init/runner.ps1 echo 'C:\actions-runner\run.cmd --jitconfig '"$runner_jitconfig"
