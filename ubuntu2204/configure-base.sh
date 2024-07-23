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
download "$cache_dir" https://github.com/actions/runner/releases/download/v2.316.1/actions-runner-linux-x64-2.316.1.tar.gz d62de2400eeeacd195db91e2ff011bfb646cd5d85545e81d8f78c436183e09a8

>&2 echo '[*] Injecting init script'
mkdir -p init
inject etc/rc.local "$script_dir/ubuntu2204/init.sh"
inject init "$cache_dir/rustup-init.sh"
inject init "$cache_dir/rustup-init.sh"
chmod +x init/rustup-init.sh

>&2 echo '[*] Injecting GitHub Actions runner'
# See also: <https://github.com/servo/servo/settings/actions/runners/new?arch=x64&os=linux>
rm -Rf actions-runner  # FIXME: necessary to avoid errors starting runner?
mkdir -p actions-runner
( cd actions-runner; tar xf "$cache_dir/actions-runner-linux-x64-2.316.1.tar.gz" )

>&2 echo '[*] Creating working directory for builds (C:\a)'
mkdir -p a

>&2 echo '[*] Injecting servo repo'
mkdir -p a/servo
inject a/servo /mnt/servo0/servo
git -C a/servo/servo remote remove origin || :
git -C a/servo/servo remote add origin https://github.com/servo/servo.git

>&2 echo '[*] Injecting cargo cache'
inject root /mnt/servo0/.cargo
