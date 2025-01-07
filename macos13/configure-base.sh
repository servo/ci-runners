#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
cache_dir=$script_dir/cache
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$cache_dir"
download "$cache_dir" https://static.rust-lang.org/rustup/rustup-init.sh 32a680a84cf76014915b3f8aa44e3e40731f3af92cd45eb0fcc6264fd257c428
download "$cache_dir" https://github.com/actions/runner/releases/download/v2.321.0/actions-runner-osx-x64-2.321.0.tar.gz b2c91416b3e4d579ae69fc2c381fc50dbda13f1b3fcc283187e2c75d1b173072
download "$cache_dir" https://astral.sh/uv/install.sh 47ead06f79eba7461fd113fc92dc0f191af7455418462fbbed21affa2a6c22e2
download "$cache_dir/homebrew" https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh 9ad0c8048f3f1a01d5f6610e0df347ceeae5879cf0aa51c1d987aa8aee740dca

>&2 echo '[*] Injecting init script'
mkdir -p init
inject_exfat init "$script_dir/macos13/init.sh"
inject_exfat init "$cache_dir/rustup-init.sh"
inject_exfat init/install-uv.sh "$cache_dir/install.sh"
inject_exfat init/install-homebrew.sh "$cache_dir/homebrew/install.sh"
chmod +x init/rustup-init.sh init/install-uv.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
# Runner tarball includes symlinks, which are not supported by exFAT (on macOS at least)
inject_exfat init/actions-runner-osx-x64.tar.gz "$cache_dir/actions-runner-osx-x64-2.321.0.tar.gz"

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
