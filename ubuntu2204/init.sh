#!/bin/bash
set -euo pipefail

set -- git curl python3-pip python3-venv  # Bootstrap tools
set -- "$@" libssl-dev  # taplo-cli -> openssl-sys -> openssl.pc
set -- "$@" xvfb  # linux.yml -> xvfb-run
set -- "$@" python-is-python3  # Install Python, for checkouts without servo#34504
set -- "$@" fonts-noto-color-emoji  # FIXME: 2 tests require this <https://github.com/servo/servo/issues/35030>
set -- "$@" fonts-noto-cjk  # FIXME: 3 tests require this <https://github.com/servo/servo/pull/34770#issuecomment-2647805573>
set -- "$@" jq  # Used further below

# Install distro packages, but only if one or more are not already installed.
# Update the package lists first, to avoid failures when rebaking old images.
if ! dpkg -s "$@" > /dev/null 2>&1; then
    apt update
    apt install -y "$@"
fi

# Install rustup and the latest Rust
if ! [ -e /root/.rustup ]; then
    /init/rustup-init -y --quiet
fi

# ~/.cargo/env requires HOME to be set
export HOME=/root
. /root/.cargo/env

# FIXME: 17 tests require this
# <https://github.com/servo/servo/issues/35029>
sudo apt purge -y fonts-droid-fallback

# Install uv and ensure it is on PATH
if ! [ -e /root/.local/bin/uv ]; then
    /init/uv-installer.sh
fi
export PATH=$HOME/.local/bin:$PATH

if ! [ -e /init/built_servo_once_successfully ]; then
    cd /a/servo/servo

    # Install the Rust toolchain, for checkouts without servo#35795
    rustup show active-toolchain || rustup toolchain install

    ./mach bootstrap --force
    # Build the same way as a typical Linux libservo job, to allow for incremental builds.
    cargo build -p libservo --all-targets --release --target-dir target/libservo
    # Build the same way as a typical Linux build job, to allow for incremental builds.
    ./mach build --use-crown --locked --release
    # Some hacks that seem to help with incremental builds.
    git status
    ./mach build --use-crown --locked --release
    touch /init/built_servo_once_successfully
    poweroff
    exit
else
    cd /a/servo/servo
    # Freshen git’s understanding of the working tree.
    git status
    # Freshen cargo’s understanding of the incremental build.
    ./mach build --use-crown --locked --release
fi

> /init/runner.sh echo 'export RUNNER_ALLOW_RUNASROOT=1'
>> /init/runner.sh printf '/actions-runner/run.sh --jitconfig '
curl -fsS http://192.168.100.1:8000/github-jitconfig | jq -er . >> /init/runner.sh
chmod +x /init/runner.sh
/init/runner.sh  # Only runs if curl and jq succeeded
