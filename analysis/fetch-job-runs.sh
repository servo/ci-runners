#!/usr/bin/env zsh
# usage: fetch-job-runs.sh <path/to/workflow-runs.json> > job-runs.json
# requires: zsh, gh, jq, rg
set -euo pipefail
if [ $# -lt 1 ]; then >&2 sed '1d;2s/^# //;2q' "$0"; exit 1; fi
missing() { >&2 echo "fatal: $1 not found"; exit 1; }
> /dev/null command -v gh || missing gh
> /dev/null command -v jq || missing jq
> /dev/null command -v rg || missing rg
cd -- "$(dirname -- "${0:a}")"
workflow_runs=$1; shift

total_requests_needed=$(jq length "$workflow_runs")
i=1; jq -r '.jobs_url' "$workflow_runs" | while read -r jobs_url; do
  >&2 printf '%s/%s\n' $i $total_requests_needed
  gh api "$jobs_url" -q '.jobs[]'
  i=$((i+1))
done
