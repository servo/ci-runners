#!/bin/bash

uv tool run --from mitmproxy mitmdump -v --server-replay /mitmdump/dump-current --set server_replay_extra=404 --set server_replay_reuse=true &

# we only ever use "--jitconfig ..." so two arguments is enough.
/home/servo_ci/runner/run.sh $1 $2 &

wait -n

# Exit with status of process that exited first
exit $?
