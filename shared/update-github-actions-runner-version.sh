#!/bin/sh

usage() {
    echo "Usage: $0 GHA_VERSION" >&2
    exit 1
}

NEW_VERSION=$1
[ -n "$NEW_VERSION" ] || usage

SCRIPT_DIR=$(dirname "$0")

get_hash () {
    digest=$(gh api "repos/actions/runner/releases/tags/v$NEW_VERSION" --jq '.assets[] | select(.name | startswith("actions-runner-'"$1"'-x64-")) | .digest') || return 1
    [ -n "$digest" ] || return 1
    echo "$digest" | sed 's/^sha256://' | xxd -r -p | base64 | sed 's/^/sha256-/'
}

LINUX_HASH=$(get_hash linux) || { echo "No hash found for Linux" >&2; exit 1; }
WINDOWS_HASH=$(get_hash win) || { echo "No hash found for Windows" >&2; exit 1; }
MACOS_HASH=$(get_hash osx) || { echo "No hash found for macOS" >&2; exit 1; }

cat > "$SCRIPT_DIR/github-actions-runner.json" <<EOF
{
    "version": "$NEW_VERSION",
    "hashes": {
        "linux-x64": "$LINUX_HASH",
        "osx-x64": "$MACOS_HASH",
        "win-x64": "$WINDOWS_HASH"
    }
}
EOF
