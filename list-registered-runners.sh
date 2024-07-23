#!/usr/bin/env zsh
# usage: list-registered-runners.sh
script_dir=${0:a:h}
. "$script_dir/common.sh"

gh api -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" \
    "$SERVO_CI_GITHUB_API_SCOPE/actions/runners" \
    --paginate -q '.runners[]' \
    | jq -s .
