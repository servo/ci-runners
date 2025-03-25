#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$SERVO_CI_CACHE_PATH"
download "$SERVO_CI_CACHE_PATH" https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-apple-darwin/rustup-init e4b1f9ec613861232247e0cb6361c9bb1a86525d628ecd4b9feadc9ef9e0c228
download "$SERVO_CI_CACHE_PATH" https://github.com/actions/runner/releases/download/v2.323.0/actions-runner-osx-x64-2.323.0.tar.gz 5dd3f423e8f387a47ac53a5e355e0fe105f0a9314d7823dea098dca70e1bd2c9
download "$SERVO_CI_CACHE_PATH" https://github.com/astral-sh/uv/releases/download/0.6.9/uv-installer.sh f1288cc7987c8e098131e1895e18bb5e232021424e7332609ec3ded0a9509799
download "$SERVO_CI_CACHE_PATH" https://raw.githubusercontent.com/actions/runner-images/3d5f09a90fd475a3531b0ef57325aa7e27b24595/images/macos/scripts/build/install-xcode-clt.sh 2c90d2c76f2d375ef5404d67510894ebbf917940aef5017ebee4c6b8c10c42fb
download "$SERVO_CI_CACHE_PATH/homebrew" https://raw.githubusercontent.com/Homebrew/install/9a01f1f361cc66159c31624df04b6772d26b7f98/install.sh a30b9fbf0d5c2cff3eb1d0643cceee30d8ba6ea1bb7bcabf60d3188bd62e6ba6

>&2 echo '[*] Injecting init script'
mkdir -p init
inject_exfat init "$script_dir/macos13/init.sh"
inject_exfat init "$SERVO_CI_CACHE_PATH/rustup-init"
inject_exfat init "$SERVO_CI_CACHE_PATH/uv-installer.sh"
inject_exfat init "$SERVO_CI_CACHE_PATH/install-xcode-clt.sh"
inject_exfat init/install-homebrew.sh "$SERVO_CI_CACHE_PATH/homebrew/install.sh"
chmod +x init/rustup-init init/uv-installer.sh init/install-xcode-clt.sh init/install-homebrew.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
# Runner tarball includes symlinks, which are not supported by exFAT (on macOS at least)
inject_exfat init/actions-runner-osx-x64.tar.gz "$SERVO_CI_CACHE_PATH/actions-runner-osx-x64-2.323.0.tar.gz"

>&2 echo '[*] Creating working directory for builds (/Volumes/a/a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject_exfat a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

>&2 echo '[*] Injecting cargo cache'
inject_exfat . "$SERVO_CI_DOT_CARGO_PATH"

>&2 echo '[*] Injecting cargo config'
inject_exfat .cargo/config.toml "$script_dir/shared/cargo-config.toml"
