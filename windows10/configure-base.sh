#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$SERVO_CI_CACHE_PATH"
download "$SERVO_CI_CACHE_PATH" https://www.python.org/ftp/python/3.10.11/python-3.10.11-amd64.exe d8dede5005564b408ba50317108b765ed9c3c510342a598f9fd42681cbe0648b
download "$SERVO_CI_CACHE_PATH" https://github.com/astral-sh/uv/releases/download/0.6.10/uv-installer.ps1 95614458784a7898fba2de1704a986a930a8b1cf8f6512f28e77546e517269e1
download "$SERVO_CI_CACHE_PATH" https://download.visualstudio.microsoft.com/download/pr/2d6bb6b2-226a-4baa-bdec-798822606ff1/8494001c276a4b96804cde7829c04d7f/ndp48-x86-x64-allos-enu.exe 68c9986a8dcc0214d909aa1f31bee9fb5461bb839edca996a75b08ddffc1483f
download "$SERVO_CI_CACHE_PATH" https://github.com/microsoft/vswhere/releases/download/3.1.7/vswhere.exe c54f3b7c9164ea9a0db8641e81ecdda80c2664ef5a47c4191406f848cc07c662
download "$SERVO_CI_CACHE_PATH" https://aka.ms/vs/17/release/vs_community.exe cd87b7e84c0b9dc0ae9aaf1cbff518e48b8c1e3757712354eab37677649bdcef
download "$SERVO_CI_CACHE_PATH" https://static.rust-lang.org/rustup/archive/1.28.1/x86_64-pc-windows-msvc/rustup-init.exe 7b83039a1b9305b0c50f23b2e2f03319b8d7859b28106e49ba82c06d81289df6
download "$SERVO_CI_CACHE_PATH" https://github.com/actions/runner/releases/download/v2.323.0/actions-runner-win-x64-2.323.0.zip e8ca92e3b1b907cdcc0c94640f4c5b23f377743993a4a5c859cb74f3e6eb33ef
download "$SERVO_CI_CACHE_PATH" https://github.com/git-for-windows/git/releases/download/v2.45.1.windows.1/Git-2.45.1-64-bit.exe 1b2b58fb516495feb70353aa91da230be0a2b4aa01acc3bc047ee1fe4846bc4e

>&2 echo '[*] Applying changes to SOFTWARE hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SOFTWARE' Windows/System32/config/SOFTWARE < "$script_dir/windows10/software.reg"

>&2 echo '[*] Applying changes to SYSTEM hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SYSTEM' Windows/System32/config/SYSTEM < "$script_dir/windows10/system.reg"

>&2 echo '[*] Injecting init script and installers'
mkdir -p init
inject init "$script_dir/windows10/init.ps1"
inject init "$script_dir/windows10/warm.ps1"
inject init "$script_dir/windows10/refreshenv.ps1"
inject init "$SERVO_CI_CACHE_PATH/python-3.10.11-amd64.exe"
inject init "$SERVO_CI_CACHE_PATH/uv-installer.ps1"
inject init "$SERVO_CI_CACHE_PATH/ndp48-x86-x64-allos-enu.exe"
inject init "$SERVO_CI_CACHE_PATH/vswhere.exe"
inject init "$SERVO_CI_CACHE_PATH/vs_community.exe"
inject init "$SERVO_CI_CACHE_PATH/rustup-init.exe"
inject init "$SERVO_CI_CACHE_PATH/Git-2.45.1-64-bit.exe"

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=win>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
unzip -o -d actions-runner "$SERVO_CI_CACHE_PATH/actions-runner-win-x64-2.323.0.zip"

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

# `git clone` would have set this if run directly on our Windows runners, and
# not having it makes the working tree constantly look dirty to git, with files
# that should have git mode 100755 actually having git mode 100644.
git -C a/servo/servo config core.fileMode false

>&2 echo '[*] Injecting cargo cache'
inject Users/Administrator "$SERVO_CI_DOT_CARGO_PATH"

>&2 echo '[*] Injecting cargo config'
inject Users/Administrator/.cargo/config.toml "$script_dir/shared/cargo-config.toml"
