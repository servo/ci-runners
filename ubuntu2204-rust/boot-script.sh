#!/bin/sh
set -eux

actions_runner_version=2.323.0

download() (
    set -x
    curl -fsSLO "http://192.168.100.1:8000/image-deps/ubuntu2204/$1"
)

mkdir -p /ci
cd /ci

if ! [ -e image-expanded ]; then
    touch image-expanded
    reboot
fi

set -- jq  # Used further below

# Install distro packages, but only if one or more are not already installed.
# Update the package lists first, to avoid failures when rebaking old images.
if ! dpkg -s "$@" > /dev/null 2>&1; then
    apt update
    # DEBIAN_FRONTEND needed to avoid hang when installing tshark
    DEBIAN_FRONTEND=noninteractive apt install -y "$@"
fi

if ! [ -e actions-runner-linux-x64-$actions_runner_version.tar.gz ]; then
    download actions-runner-linux-x64-$actions_runner_version.tar.gz
    rm -Rf actions-runner
    mkdir -p actions-runner
    ( cd actions-runner; tar xf ../actions-runner-linux-x64-$actions_runner_version.tar.gz )
fi

if ! [ -e image-built ]; then
    touch image-built
    poweroff
fi

export RUNNER_ALLOW_RUNASROOT=1
curl -fsS --max-time 5 --retry 99 --retry-all-errors http://192.168.100.1:8000/github-jitconfig | jq -er . > jitconfig
actions-runner/run.sh --jitconfig $(cat jitconfig)
