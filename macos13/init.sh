#!/usr/bin/env zsh
set -euo pipefail -o bsdecho

# Resize the window to occupy more of the 1280x800 display
# - Method based on <https://apple.stackexchange.com/a/290802>
# - Another method for exclusive fullscreen <https://apple.stackexchange.com/a/58962>
# - Another method with unclear automation <https://apple.stackexchange.com/a/228052>
osascript -e 'tell application "Terminal"' -e 'activate' -e 'set the bounds of the first window to {0,0,1280,600}' -e 'end tell'

# Disable sleep and display sleep
# <https://apple.stackexchange.com/a/458157>
sudo pmset sleep 0
sudo pmset displaysleep 0

# Install Xcode CLT (Command Line Tools) non-interactively
# <https://github.com/actions/runner-images/blob/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh>
sudo -i mkdir -p /var/root/utils
sudo -i touch /var/root/utils/utils.sh
sudo -i /Volumes/a/init/install-xcode-clt.sh

# Install Homebrew
if ! [ -e /usr/local/bin/brew ]; then
    NONINTERACTIVE=1 /Volumes/a/init/install-homebrew.sh
fi

# Install gtar(1)
brew install gnu-tar

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

    # Install the Rust toolchain, for checkouts without servo#35795
    rustup show active-toolchain || rustup toolchain install

    ./mach bootstrap --force
    # Build the same way as a typical macOS libservo job, to allow for incremental builds.
    cargo build -p libservo --all-targets --release --target-dir target/libservo
    # Build the same way as a typical macOS build job, to allow for incremental builds.
    ./mach build --use-crown --locked --release
    touch /Volumes/a/init/built_servo_once_successfully
    sudo shutdown -h now
    exit
fi

if [ -e /Volumes/a/init/runner.sh ]; then
    . /Volumes/a/init/runner.sh
fi
