#!/usr/bin/env zsh
set -xeuo pipefail -o bsdecho
script_dir=${0:a:h}
cd "$script_dir/../../intermittent-tracker"

systemctl stop intermittent-tracker-staging
cp -v prod/data/* staging/data/
systemctl start intermittent-tracker-staging
