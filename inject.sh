#!/usr/bin/env zsh
# usage once sourced: inject <dest> <src> [src ...]
set -euo pipefail -o bsdecho

inject() (
    dest=$1; shift
    rsync -a --no-i-r --info=progress2 -- "$@" "$dest"
)
