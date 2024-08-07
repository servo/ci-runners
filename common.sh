#!/usr/bin/env zsh
# usage: script_dir=${0:a:h}; . "$script_dir/common.sh"
set -euo pipefail -o bsdecho

# Set and export variables from .env.
set -a
. $script_dir/.env
set +a

export SERVO_CI_MONITOR_DATA_PATH=${SERVO_CI_MONITOR_DATA_PATH-$script_dir/monitor/data}
