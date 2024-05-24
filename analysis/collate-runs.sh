#!/usr/bin/env zsh
# usage: collate-runs.sh <path/to/workflow-runs.json> <path/to/job-runs.json> > runs.json
# requires: zsh, gh, jq, rg
set -euo pipefail
if [ $# -lt 1 ]; then >&2 sed '1d;2s/^# //;2q' "$0"; exit 1; fi
missing() { >&2 echo "fatal: $1 not found"; exit 1; }
> /dev/null command -v gh || missing gh
> /dev/null command -v jq || missing jq
> /dev/null command -v rg || missing rg
cd -- "$(dirname -- "${0:a}")"
workflow_runs=$1; shift
job_runs=$1; shift

< "$workflow_runs" jq --slurpfile job_runs "$job_runs" -s '
  map(select(.run_attempt == 1 and .previous_attempt_url == null and .conclusion == "success"))
  | reduce .[] as $workflow_run ({}; . + {($workflow_run.url): {workflow_run: $workflow_run, job_runs: (
    $job_runs | map(select(.run_url == $workflow_run.url))   # O(n^2)
  )}})
'
