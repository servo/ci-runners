#!/usr/bin/env zsh
set -euo pipefail
gh_api_cached() {
    local url=$1
    local cache=cache_${url:gs/\//_}
    if [ -e "$cache" ]; then
        cat -- "$cache"
    else
        gh api "$url" | tee -- "$cache.tmp"
        mv -- "$cache.tmp" "$cache"
    fi
}
page_number=1
while :; do
    gh api '/repos/servo/servo/actions/runs?per_page=100&page='$page_number \
    | jq -er '.workflow_runs[] | select((.updated_at | fromdateiso8601) - (.created_at | fromdateiso8601) >= 120) | .jobs_url' \
    | while read -r url; do
        printf '>>> %s\n' "$url"
        # `name` ends with `]` when the job has a unique id in its name,
        # which is in turn present because it’s needed for runner-timeout.
        # `runner_group_name` is:
        # - `null` if the job was skipped
        # - `"default"` if the job got a self-hosted runner
        # - `"GitHub Actions"` if the job got a GitHub-hosted runner
        # - `""` if the job was cancelled or still waiting for a runner
        gh_api_cached "$url" \
        | jq -c '.jobs[] | select(.name | endswith("]")) | select(.runner_group_name != null) | {self_hosted: .runner_group_name == "default", github_hosted: .runner_group_name == "GitHub Actions", run_id, id, name}'
    done
    page_number=$((page_number + 1))
done
