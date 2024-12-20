#!/usr/bin/env zsh
# usage: script_dir=${0:a:h}; . "$script_dir/common.sh"
set -euo pipefail -o bsdecho

# Set and export variables from .env.
set -a
. $script_dir/.env
set +a

export SERVO_CI_MONITOR_DATA_PATH=${SERVO_CI_MONITOR_DATA_PATH-$script_dir/monitor/data}

# usage: trap print_undo_commands EXIT
print_undo_commands() {
    exit_status=$?
    if [ $exit_status -ne 0 ]; then
        >&2 echo
        >&2 echo "Failed to build image!"
    fi
    if [ -n "$(cat $undo_commands)" ]; then
        >&2 echo
        >&2 echo "[*] To abort:"
        >&2 tac $undo_commands
        exit $exit_status
    fi
}
