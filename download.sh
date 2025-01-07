#!/usr/bin/env zsh
# usage once sourced: download <dest_dir> <url> [sha256]
set -euo pipefail -o bsdecho

download() {
    local dest_path=$1/${2##*/}
    local expected=${3-0000000000000000000000000000000000000000000000000000000000000000}
    mkdir -p -- "$1"
    [ -e "$dest_path" ] || curl -Lo "$dest_path" -- "$2"
    if ! printf '%s  %s\n' "$expected" "$dest_path" | sha256sum -c; then
        >&2 printf 'Expected sha256: %s\n' "$expected"
        >&2 printf 'Actual sha256:   %s\n' "$(sha256sum "$dest_path" | cut -d' ' -f1)"
        exit 1
    fi
}
