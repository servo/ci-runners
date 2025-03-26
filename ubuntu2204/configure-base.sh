#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Injecting init script'
mkdir -p init
inject_regular_file etc/rc.local "$script_dir/ubuntu2204/init.sh"
inject_regular_file init "$IMAGE_DEPS_DIR/ubuntu2204/rustup-init"
inject_regular_file init "$IMAGE_DEPS_DIR/ubuntu2204/uv-installer.sh"
chmod +x init/rustup-init init/uv-installer.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
( cd actions-runner; tar xf "$IMAGE_DEPS_DIR/ubuntu2204/actions-runner-linux-x64-2.323.0.tar.gz" )

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject_dir_recursive a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

>&2 echo '[*] Injecting cargo cache'
inject_dir_recursive root "$SERVO_CI_DOT_CARGO_PATH"

>&2 echo '[*] Injecting cargo config'
inject_regular_file root/.cargo/config.toml "$script_dir/shared/cargo-config.toml"
