#!/usr/bin/env zsh
# usage: configure-base.sh
script_dir=${0:a:h}/..
. "$script_dir/common.sh"
cache_dir=$script_dir/cache
. "$script_dir/download.sh"
. "$script_dir/inject.sh"

>&2 echo '[*] Caching downloads'
mkdir -p -- "$cache_dir"
download "$cache_dir" https://www.python.org/ftp/python/3.12.3/python-3.12.3-amd64.exe edfc6c84dc47eebd4fae9167e96ff5d9c27f8abaa779ee1deab9c3d964d0de3c
download "$cache_dir" https://download.visualstudio.microsoft.com/download/pr/2d6bb6b2-226a-4baa-bdec-798822606ff1/8494001c276a4b96804cde7829c04d7f/ndp48-x86-x64-allos-enu.exe 68c9986a8dcc0214d909aa1f31bee9fb5461bb839edca996a75b08ddffc1483f
download "$cache_dir" https://github.com/microsoft/vswhere/releases/download/3.1.7/vswhere.exe c54f3b7c9164ea9a0db8641e81ecdda80c2664ef5a47c4191406f848cc07c662
download "$cache_dir" https://aka.ms/vs/17/release/vs_community.exe 5606944c31b01519f5932cdfa29f1cf1c2591a7ebe973987bd03504dbcc0bbf9
download "$cache_dir" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe 193d6c727e18734edbf7303180657e96e9d5a08432002b4e6c5bbe77c60cb3e8
download "$cache_dir" https://github.com/actions/runner/releases/download/v2.319.1/actions-runner-win-x64-2.319.1.zip 1c78c51d20b817fb639e0b0ab564cf0469d083ad543ca3d0d7a2cdad5723f3a7
download "$cache_dir" https://github.com/git-for-windows/git/releases/download/v2.45.1.windows.1/Git-2.45.1-64-bit.exe 1b2b58fb516495feb70353aa91da230be0a2b4aa01acc3bc047ee1fe4846bc4e

>&2 echo '[*] Applying changes to SOFTWARE hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SOFTWARE' Windows/System32/config/SOFTWARE < "$script_dir/windows10/software.reg"

>&2 echo '[*] Applying changes to SYSTEM hive'
hivexregedit --merge --prefix 'HKEY_LOCAL_MACHINE\SYSTEM' Windows/System32/config/SYSTEM < "$script_dir/windows10/system.reg"

>&2 echo '[*] Injecting init script and installers'
mkdir -p init
inject init "$script_dir/windows10/init.ps1"
inject init "$script_dir/windows10/warm.ps1"
inject init "$script_dir/windows10/refreshenv.ps1"
inject init "$cache_dir/python-3.12.3-amd64.exe"
inject init "$cache_dir/ndp48-x86-x64-allos-enu.exe"
inject init "$cache_dir/vswhere.exe"
inject init "$cache_dir/vs_community.exe"
inject init "$cache_dir/rustup-init.exe"
inject init "$cache_dir/Git-2.45.1-64-bit.exe"

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=win>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
unzip -o -d actions-runner "$cache_dir/actions-runner-win-x64-2.319.1.zip"

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject a/servo "$SERVO_CI_MAIN_REPO_PATH"
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

>&2 echo '[*] Injecting cargo cache'
inject Users/Administrator "$SERVO_CI_DOT_CARGO_PATH"
