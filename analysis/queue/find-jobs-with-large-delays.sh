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
        gh_api_cached "$url" \
        | jq -c '.jobs[] | select(.name | contains("runner-select")) | {delay: (.started_at | fromdateiso8601) - (.created_at | fromdateiso8601), run_id, id, name} | select(.delay >= 30)'
    done
    page_number=$((page_number + 1))
done
