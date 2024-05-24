#!/usr/bin/env zsh
# usage: compute-critical-path.sh <path/to/collated-runs.json> <first job names json> <last job names json>
# requires: zsh, gh, jq, rg
set -euo pipefail
if [ $# -lt 1 ]; then >&2 sed '1d;2s/^# //;2q' "$0"; exit 1; fi
missing() { >&2 echo "fatal: $1 not found"; exit 1; }
> /dev/null command -v gh || missing gh
> /dev/null command -v jq || missing jq
> /dev/null command -v rg || missing rg
cd -- "$(dirname -- "${0:a}")"
collated_runs=$1; shift
first_job_names=$1; shift
last_job_names=$1; shift

jq_program='
  ($firsts | unique) as $firsts
  | ($lasts | unique) as $lasts
  | map_values(
      .job_runs as $job_runs
      | select(
          reduce $firsts[] as $first (
            false;
            . or ($job_runs | map(select(.name == $first)) | length) > 0
          ) and reduce $lasts[] as $last (
            false;
            . or ($job_runs | map(select(.name == $last)) | length) > 0
          )
        )
    )
  | map_values(
      .job_runs as $job_runs
      | (reduce $firsts[] as $first (
          []; . + ($job_runs | map(select(.name == $first)))
        )) as $first_jobs
      | (reduce $lasts[] as $last (
          []; . + ($job_runs | map(select(.name == $last)))
        )) as $last_jobs
      | {
          workflow_run_started_at: .workflow_run.run_started_at | fromdateiso8601,
          first_job_names: [$first_jobs[].name],
          last_job_names: [$last_jobs[].name],
          first_job_times: [$first_jobs[].started_at | fromdateiso8601] | sort,
          last_job_times: [$last_jobs[].completed_at | fromdateiso8601] | sort,
          first_job_min_started: [$first_jobs[].started_at | fromdateiso8601] | min,
          first_job_max_started: [$first_jobs[].started_at | fromdateiso8601] | max,
          last_job_max_completed: [$last_jobs[].completed_at | fromdateiso8601] | max,
        }
      | {
          duration: (.last_job_max_completed - .first_job_min_started),
          max_started_at: (.first_job_max_started - .workflow_run_started_at),
          max_completed_at: (.last_job_max_completed - .workflow_run_started_at),
        }
    )
  | {
      duration: map(.duration) | [add / length / 60, max / 60, min / 60],
      max_started_at: map(.max_started_at) | [add / length / 60, max / 60, min / 60],
      max_completed_at: map(.max_completed_at) | [add / length / 60, max / 60, min / 60],
    }
'

< "$collated_runs" jq "$jq_program" \
  --argjson firsts "$first_job_names" \
  --argjson lasts "$last_job_names"
