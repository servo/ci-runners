#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Applying changes to SOFTWARE hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SOFTWARE' Windows/System32/config/SOFTWARE < "$script_dir/windows10/software.reg"

>&2 echo '[*] Applying changes to SYSTEM hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SYSTEM' Windows/System32/config/SYSTEM < "$script_dir/windows10/system.reg"

>&2 echo '[*] Injecting init script and installers'
mkdir -p init
inject_regular_file init "$script_dir/windows10/init.ps1"
inject_regular_file init "$script_dir/windows10/warm.ps1"
inject_regular_file init "$script_dir/windows10/refreshenv.ps1"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/python-3.10.11-amd64.exe"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/uv-installer.ps1"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/ndp48-x86-x64-allos-enu.exe"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/vswhere.exe"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/vs_community.exe"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/rustup-init.exe"
inject_regular_file init "$IMAGE_DEPS_DIR/windows10/Git-2.45.1-64-bit.exe"

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=win>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
unzip -o -d actions-runner "$IMAGE_DEPS_DIR/windows10/actions-runner-win-x64-2.323.0.zip"

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject_dir_recursive a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

# `git clone` would have set this if run directly on our Windows runners, and
# not having it makes the working tree constantly look dirty to git, with files
# that should have git mode 100755 actually having git mode 100644.
git -C a/servo/servo config core.fileMode false

>&2 echo '[*] Injecting cargo cache'
inject_dir_recursive Users/Administrator "$SERVO_CI_DOT_CARGO_PATH"

>&2 echo '[*] Injecting cargo config'
inject_regular_file Users/Administrator/.cargo/config.toml "$script_dir/shared/cargo-config.toml"
