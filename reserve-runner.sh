#!/usr/bin/env zsh
# usage: reserve-runner.sh <github_runner_id> <unique_id> <reserved_by>
script_dir=${0:a:h}
. "$script_dir/common.sh"
github_runner_id=$1; shift
unique_id=$1; shift
reserved_by=$1; shift

reserved_since=$(date +\%s)
gh api "$SERVO_CI_GITHUB_API_SCOPE/actions/runners/$github_runner_id/labels" \
    -f "labels[]=reserved-for:$unique_id" \
    -f "labels[]=reserved-since:$reserved_since" \
    -f "labels[]=reserved-by:$reserved_by" \
    --method POST
