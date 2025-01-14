#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$SERVO_CI_CACHE_PATH"
download "$SERVO_CI_CACHE_PATH" https://www.python.org/ftp/python/3.10.11/python-3.10.11-amd64.exe d8dede5005564b408ba50317108b765ed9c3c510342a598f9fd42681cbe0648b
download "$SERVO_CI_CACHE_PATH" https://astral.sh/uv/install.ps1 87fe546b1fd64d0f2d776a185fa52ec786e0f0624f359480781f724123604362
download "$SERVO_CI_CACHE_PATH" https://download.visualstudio.microsoft.com/download/pr/2d6bb6b2-226a-4baa-bdec-798822606ff1/8494001c276a4b96804cde7829c04d7f/ndp48-x86-x64-allos-enu.exe 68c9986a8dcc0214d909aa1f31bee9fb5461bb839edca996a75b08ddffc1483f
download "$SERVO_CI_CACHE_PATH" https://github.com/microsoft/vswhere/releases/download/3.1.7/vswhere.exe c54f3b7c9164ea9a0db8641e81ecdda80c2664ef5a47c4191406f848cc07c662
download "$SERVO_CI_CACHE_PATH" https://aka.ms/vs/17/release/vs_community.exe 5606944c31b01519f5932cdfa29f1cf1c2591a7ebe973987bd03504dbcc0bbf9
download "$SERVO_CI_CACHE_PATH" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe 193d6c727e18734edbf7303180657e96e9d5a08432002b4e6c5bbe77c60cb3e8
download "$SERVO_CI_CACHE_PATH" https://github.com/actions/runner/releases/download/v2.321.0/actions-runner-win-x64-2.321.0.zip 88d754da46f4053aec9007d172020c1b75ab2e2049c08aef759b643316580bbc
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
inject init/install-uv.ps1 "$SERVO_CI_CACHE_PATH/install.ps1"
inject init "$SERVO_CI_CACHE_PATH/ndp48-x86-x64-allos-enu.exe"
inject init "$SERVO_CI_CACHE_PATH/vswhere.exe"
inject init "$SERVO_CI_CACHE_PATH/vs_community.exe"
inject init "$SERVO_CI_CACHE_PATH/rustup-init.exe"
inject init "$SERVO_CI_CACHE_PATH/Git-2.45.1-64-bit.exe"

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=win>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
unzip -o -d actions-runner "$SERVO_CI_CACHE_PATH/actions-runner-win-x64-2.321.0.zip"

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
