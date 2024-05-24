#!/usr/bin/env zsh
# usage: fetch-workflow-runs.sh <org/repo> <created date regex> > workflow-runs.json
#   e.g. fetch-workflow-runs.sh servo/servo '2024-04-.*' > 2024-04.json
# requires: zsh, gh, jq, rg
set -euo pipefail
if [ $# -lt 2 ]; then >&2 sed '1d;2s/^# //;2q' "$0"; exit 1; fi
missing() { >&2 echo "fatal: $1 not found"; exit 1; }
> /dev/null command -v gh || missing gh
> /dev/null command -v jq || missing jq
> /dev/null command -v rg || missing rg
cd -- "$(dirname -- "${0:a}")"
org_repo_slug=$1; shift
created_date_regex=$1; shift

found_any=0
i=1; while :; do
  >&2 echo page $i

  # We want to filter by created date, but the github api’s created
  # query limits the search to 1000 results after pagination >:(
  gh api '/repos/'"$org_repo_slug"'/actions/runs?per_page=100&page='$i > out.json

  # If we see a whole page of results that fail the created date regex, stop to
  # save time, despite the small chance of accidentally stopping early.
  if ! jq -r '.workflow_runs[] | .created_at' out.json | rg -q '^'"$created_date_regex"'$'; then
    if [ $found_any -eq 1 ]; then
      >&2 echo done
      break
    else
      >&2 echo no results on this page
    fi
  else
    found_any=1

    # Filter the results by the merged date regex, this time using jq.
    jq '.workflow_runs[] | select(.created_at | test("^'"$created_date_regex"'$"))' out.json
  fi

  i=$((i+1))
done
