#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$SERVO_CI_CACHE_PATH"
download "$SERVO_CI_CACHE_PATH/rustup-linux" https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-unknown-linux-gnu/rustup-init a3339fb004c3d0bb9862ba0bce001861fe5cbde9c10d16591eb3f39ee6cd3e7f
download "$SERVO_CI_CACHE_PATH" https://github.com/actions/runner/releases/download/v2.323.0/actions-runner-linux-x64-2.323.0.tar.gz 0dbc9bf5a58620fc52cb6cc0448abcca964a8d74b5f39773b7afcad9ab691e19
download "$SERVO_CI_CACHE_PATH" https://github.com/astral-sh/uv/releases/download/0.6.9/uv-installer.sh f1288cc7987c8e098131e1895e18bb5e232021424e7332609ec3ded0a9509799

>&2 echo '[*] Injecting init script'
mkdir -p init
inject etc/rc.local "$script_dir/ubuntu2204/init.sh"
inject init "$SERVO_CI_CACHE_PATH/rustup-linux/rustup-init"
inject init "$SERVO_CI_CACHE_PATH/uv-installer.sh"
chmod +x init/rustup-init init/uv-installer.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
( cd actions-runner; tar xf "$SERVO_CI_CACHE_PATH/actions-runner-linux-x64-2.323.0.tar.gz" )

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

>&2 echo '[*] Injecting cargo cache'
inject root "$SERVO_CI_DOT_CARGO_PATH"

>&2 echo '[*] Injecting cargo config'
inject root/.cargo/config.toml "$script_dir/shared/cargo-config.toml"
