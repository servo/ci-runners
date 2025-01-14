#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$SERVO_CI_CACHE_PATH"
download "$SERVO_CI_CACHE_PATH" https://static.rust-lang.org/rustup/rustup-init.sh 32a680a84cf76014915b3f8aa44e3e40731f3af92cd45eb0fcc6264fd257c428
download "$SERVO_CI_CACHE_PATH" https://github.com/actions/runner/releases/download/v2.321.0/actions-runner-linux-x64-2.321.0.tar.gz ba46ba7ce3a4d7236b16fbe44419fb453bc08f866b24f04d549ec89f1722a29e
download "$SERVO_CI_CACHE_PATH" https://astral.sh/uv/install.sh 47ead06f79eba7461fd113fc92dc0f191af7455418462fbbed21affa2a6c22e2

>&2 echo '[*] Injecting init script'
mkdir -p init
inject etc/rc.local "$script_dir/ubuntu2204/init.sh"
inject init "$SERVO_CI_CACHE_PATH/rustup-init.sh"
inject init/install-uv.sh "$SERVO_CI_CACHE_PATH/install.sh"
chmod +x init/rustup-init.sh init/install-uv.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
( cd actions-runner; tar xf "$SERVO_CI_CACHE_PATH/actions-runner-linux-x64-2.321.0.tar.gz" )

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
