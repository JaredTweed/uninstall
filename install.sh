#!/bin/sh
set -eu

RELEASE_VERSION=0.7.0
SOURCE_URL=${UNINSTALL_SOURCE_URL:-https://raw.githubusercontent.com/JaredTweed/uninstall/v${RELEASE_VERSION}/uninstall}
PREFIX=${PREFIX:-/usr/local}
DESTINATION="$PREFIX/bin/uninstall"

for required_command in chmod curl dirname install mkdir mktemp python3 rm sed; do
    if ! command -v "$required_command" >/dev/null 2>&1; then
        printf '%s\n' "$required_command is required but was not found." >&2
        exit 1
    fi
done
if ! python3 -c 'import sys; raise SystemExit(sys.version_info < (3, 8))'; then
    printf '%s\n' "Python 3.8 or newer is required." >&2
    exit 1
fi

tmp_file=$(mktemp "${TMPDIR:-/tmp}/uninstall.XXXXXX")
trap 'rm -f "$tmp_file"' EXIT HUP INT TERM

printf '%s\n' "Downloading uninstall ${RELEASE_VERSION}…"
curl -fsSL "$SOURCE_URL" -o "$tmp_file"
head_line=$(sed -n '1p' "$tmp_file")
if [ "$head_line" != '#!/usr/bin/env python3' ]; then
    printf '%s\n' "Downloaded file does not look like uninstall; refusing to install." >&2
    exit 1
fi
chmod 755 "$tmp_file"
if ! "$tmp_file" --version >/dev/null 2>&1; then
    printf '%s\n' "Downloaded file failed its self-check; refusing to install." >&2
    exit 1
fi

destination_dir=$(dirname "$DESTINATION")
if [ ! -d "$destination_dir" ]; then
    if mkdir -p "$destination_dir" 2>/dev/null; then
        :
    elif command -v sudo >/dev/null 2>&1; then
        sudo install -d -m 755 "$destination_dir"
    else
        printf '%s\n' "Cannot create $destination_dir; run as root or set PREFIX." >&2
        exit 1
    fi
fi

if [ -w "$destination_dir" ]; then
    install -m 755 "$tmp_file" "$DESTINATION"
elif command -v sudo >/dev/null 2>&1; then
    sudo install -m 755 "$tmp_file" "$DESTINATION"
else
    printf '%s\n' "Cannot write to $destination_dir; run as root or set PREFIX." >&2
    exit 1
fi

printf '%s\n' "Installed uninstall to $DESTINATION"
printf '%s\n' 'Try: uninstall FreeCAD'
