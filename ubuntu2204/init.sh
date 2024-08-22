#!/bin/bash
set -euo pipefail

apt install -y git curl python3-pip python3-venv

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
    ./mach build --release
    touch /init/built_servo_once_successfully
fi

if [ -e /init/runner.sh ]; then
    . /init/runner.sh
fi
