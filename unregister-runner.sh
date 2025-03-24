#!/usr/bin/env zsh
# usage: unregister-runner.sh <id>
script_dir=${0:a:h}
. "$script_dir/common.sh"
id=$1; shift

gh api --method DELETE -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" \
    "$SERVO_CI_GITHUB_API_SCOPE/actions/runners/$id" \
| cat  # avoid pager when attached to tty
