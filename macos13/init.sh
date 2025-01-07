#!/usr/bin/env zsh
set -euo pipefail -o bsdecho

# Install Xcode CLT (Command Line Tools) non-interactively
# <https://github.com/actions/runner-images/blob/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh>
sudo -i mkdir -p /var/root/utils
sudo -i touch /var/root/utils/utils.sh
sudo -i /Volumes/a/init/install-xcode-clt.sh

# Install Homebrew
NONINTERACTIVE=1 /Volumes/a/init/install-homebrew.sh

# Install rustup and the latest Rust
if ! [ -e /Users/servo/.rustup ]; then
    /Volumes/a/init/rustup-init.sh -y --quiet
fi

# ~/.cargo/env requires HOME to be set
export HOME=/Users/servo
. /Users/servo/.cargo/env

# Install uv and ensure it is on PATH
if ! [ -e /Users/servo/.local/bin/uv ]; then
    /Volumes/a/init/install-uv.sh
fi
export PATH=$HOME/.local/bin:$PATH

if ! [ -e /Volumes/a/init/built_servo_once_successfully ]; then
    cd /Volumes/a/a/servo/servo
    ./mach bootstrap --force
    # Build the same way as a typical Linux build job, to allow for incremental builds.
    ./mach build --use-crown --locked --release --features layout_2013
    touch /Volumes/a/init/built_servo_once_successfully
    sudo shutdown -h now
    exit
fi

if [ -e /Volumes/a/init/runner.sh ]; then
    . /Volumes/a/init/runner.sh
fi
