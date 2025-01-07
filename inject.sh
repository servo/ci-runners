#!/usr/bin/env zsh
# usage once sourced: inject <dest> <src> [src ...]
set -euo pipefail -o bsdecho

inject() (
    dest=$1; shift
    rsync -a --no-i-r --info=progress2 -- "$@" "$dest"
)

inject_exfat() (
    dest=$1; shift
    # Skip owners, groups, and links (for exFAT)
    # FIXME: are macOS Servo builds ok with this?
    rsync -a --no-o --no-g --no-l --no-i-r --info=progress2 -- "$@" "$dest"
)
