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

set -- gnu-tar  # Install gtar(1)
set -- "$@" jq  # Used further below

brew install "$@"

# Install rustup and the latest Rust
if ! [ -e /Users/servo/.rustup ]; then
    /Volumes/a/init/rustup-init -y --quiet
fi

# ~/.cargo/env requires HOME to be set
export HOME=/Users/servo
. /Users/servo/.cargo/env

# Install uv and ensure it is on PATH
if ! [ -e /Users/servo/.local/bin/uv ]; then
    /Volumes/a/init/uv-installer.sh
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

# Unpack the GitHub Actions runner on APFS (it contains symlinks)
mkdir -p ~/actions-runner
cd ~/actions-runner
tar xf /Volumes/a/init/actions-runner-osx-x64.tar.gz

> /Volumes/a/init/runner.sh echo 'export RUNNER_ALLOW_RUNASROOT=1'
>> /Volumes/a/init/runner.sh printf '~/actions-runner/run.sh --jitconfig '
curl -fsS --max-time 5 --retry 99 --retry-all-errors http://192.168.100.1:8000/github-jitconfig | jq -er . >> /Volumes/a/init/runner.sh
chmod +x /Volumes/a/init/runner.sh
/Volumes/a/init/runner.sh  # Only runs if curl and jq succeeded
