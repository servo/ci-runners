#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Injecting init script'
mkdir -p init
inject_exfat init "$script_dir/macos13/init.sh"
inject_exfat init "$IMAGE_DEPS_DIR/macos13/rustup-init"
inject_exfat init "$IMAGE_DEPS_DIR/macos13/uv-installer.sh"
inject_exfat init "$IMAGE_DEPS_DIR/macos13/install-xcode-clt.sh"
inject_exfat init "$IMAGE_DEPS_DIR/macos13/install-homebrew.sh"
chmod +x init/rustup-init init/uv-installer.sh init/install-xcode-clt.sh init/install-homebrew.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
# Runner tarball includes symlinks, which are not supported by exFAT (on macOS at least)
inject_exfat init/actions-runner-osx-x64.tar.gz "$IMAGE_DEPS_DIR/macos13/actions-runner-osx-x64-2.323.0.tar.gz"

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
