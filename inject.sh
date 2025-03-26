#!/usr/bin/env zsh
# usage once sourced: inject <dest> <src> [src ...]
set -euo pipefail -o bsdecho

inject_dir_recursive() (
    dest=$1; shift
    rsync -a --no-i-r --info=progress2 -- "$@" "$dest"
)

inject_regular_file() (
    dest=$1; shift
    rsync -aL --no-i-r --info=progress2 -- "$@" "$dest"
)

inject_exfat() (
    dest=$1; shift
    # Skip owners, groups, and links (for exFAT)
    # FIXME: are macOS Servo builds ok with this?
    rsync -aL --no-o --no-g --no-i-r --info=progress2 -- "$@" "$dest"
)
