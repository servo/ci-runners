#!/bin/bash
set -euo pipefail

# Install bootstrap tools, but only if one or more are not already installed.
# Update the package lists first, to avoid failures when rebaking old images.
set -- git curl python3-pip python3-venv
if ! dpkg -s "$@" > /dev/null 2>&1; then
    apt update
    apt install -y "$@"
fi

# Install rustup and Rust 1.80.1
if ! [ -e /root/.rustup ]; then
    /init/rustup-init.sh --default-toolchain 1.80.1 -y --quiet
fi

# ~/.cargo/env requires HOME to be set
export HOME=/root
. /root/.cargo/env

# taplo-cli -> openssl-sys -> openssl.pc
sudo apt install -y libssl-dev

# linux.yml -> xvfb-run
sudo apt install -y xvfb

if ! [ -e /init/built_servo_once_successfully ]; then
    cd /a/servo/servo
    ./mach bootstrap --force
    # Build the same way as a typical Linux build job, to allow for incremental builds.
    ./mach build --use-crown --locked --release
    touch /init/built_servo_once_successfully
fi

if [ -e /init/runner.sh ]; then
    . /init/runner.sh
fi
