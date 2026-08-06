#!/bin/sh
set -eu

RELEASE_VERSION=0.18.0
DEFAULT_SOURCE_URL="https://raw.githubusercontent.com/JaredTweed/uninstall/v${RELEASE_VERSION}/uninstall"
SOURCE_URL=${UNINSTALL_SOURCE_URL:-$DEFAULT_SOURCE_URL}
CHECKSUM_URL=${UNINSTALL_CHECKSUM_URL:-"${DEFAULT_SOURCE_URL}.sha256"}
PREFIX=${PREFIX:-/usr/local}

case "$PREFIX" in
    /*) ;;
    *) printf '%s\n' 'PREFIX must be an absolute path.' >&2; exit 1 ;;
esac
case "$PREFIX" in
    /|/bin|/boot|/dev|/etc|/lib|/lib64|/proc|/run|/sbin|/sys|/usr|/var)
        printf '%s\n' "Refusing unsafe installation PREFIX: $PREFIX" >&2
        exit 1
        ;;
esac
if [ "$(printf '%s' "$PREFIX" | tr -d '[:cntrl:]')" != "$PREFIX" ]; then
    printf '%s\n' 'PREFIX contains control characters.' >&2
    exit 1
fi

DESTINATION="$PREFIX/bin/uninstall"
for required_command in chmod curl dirname grep install mkdir mktemp mv rm sed tr uname; do
    if ! command -v "$required_command" >/dev/null 2>&1; then
        printf '%s\n' "$required_command is required but was not found." >&2
        exit 1
    fi
done

source_kind=python
if ! command -v python3 >/dev/null 2>&1 \
        || ! python3 -c 'import sys; raise SystemExit(sys.version_info < (3, 8))'; then
    if [ -n "${UNINSTALL_SOURCE_URL:-}" ]; then
        printf '%s\n' 'The selected source requires Python 3.8 or newer.' >&2
        exit 1
    fi
    machine=$(uname -m)
    case "$machine" in
        x86_64|amd64) architecture=x86_64 ;;
        aarch64|arm64) architecture=aarch64 ;;
        *) printf '%s\n' "No self-contained release is available for $machine." >&2; exit 1 ;;
    esac
    libc=glibc
    if command -v ldd >/dev/null 2>&1 \
            && ldd --version 2>&1 | grep -qi musl; then libc=musl; fi
    asset="uninstall-linux-${architecture}-${libc}"
    SOURCE_URL="https://github.com/JaredTweed/uninstall/releases/download/v${RELEASE_VERSION}/${asset}"
    CHECKSUM_URL="${SOURCE_URL}.sha256"
    source_kind=binary
fi

tmp_file=$(mktemp "${TMPDIR:-/tmp}/uninstall.XXXXXX")
checksum_file=$(mktemp "${TMPDIR:-/tmp}/uninstall-checksum.XXXXXX")
stage_file=
cleanup() {
    rm -f "$tmp_file" "$checksum_file"
    if [ -n "$stage_file" ] && [ -w "$(dirname "$stage_file")" ]; then
        rm -f "$stage_file"
    fi
}
trap cleanup EXIT HUP INT TERM

printf '%s\n' "Downloading uninstall ${RELEASE_VERSION}..."
if ! curl -fsSL "$SOURCE_URL" -o "$tmp_file" 2>/dev/null; then
    if [ "$SOURCE_URL" != "$DEFAULT_SOURCE_URL" ]; then
        exit 1
    fi
    SOURCE_URL="https://raw.githubusercontent.com/JaredTweed/uninstall/main/uninstall"
    CHECKSUM_URL="${SOURCE_URL}.sha256"
    printf '%s\n' 'Tagged source is not published yet; using the matching main-branch build.'
    curl -fsSL "$SOURCE_URL" -o "$tmp_file"
fi
if [ "$source_kind" = python ]; then
    head_line=$(sed -n '1p' "$tmp_file")
    if [ "$head_line" != '#!/usr/bin/env python3' ]; then
        printf '%s\n' 'Downloaded file does not look like uninstall; refusing to install.' >&2
        exit 1
    fi
fi

expected_checksum=${UNINSTALL_SHA256:-}
if [ -z "$expected_checksum" ] && [ -z "${UNINSTALL_SOURCE_URL:-}" ]; then
    curl -fsSL "$CHECKSUM_URL" -o "$checksum_file"
    expected_checksum=$(sed -n '1{s/[[:space:]].*//;p;}' "$checksum_file")
fi
if [ -n "$expected_checksum" ]; then
    if command -v python3 >/dev/null 2>&1; then
        actual_checksum=$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$tmp_file")
    elif command -v sha256sum >/dev/null 2>&1; then
        actual_checksum=$(sha256sum "$tmp_file" | sed 's/[[:space:]].*//')
    elif command -v openssl >/dev/null 2>&1; then
        actual_checksum=$(openssl dgst -sha256 "$tmp_file" | sed 's/^.*= //')
    else
        printf '%s\n' 'python3, sha256sum, or openssl is required to verify the download.' >&2
        exit 1
    fi
    if [ "$actual_checksum" != "$expected_checksum" ]; then
        printf '%s\n' 'Downloaded checksum mismatch; refusing to install.' >&2
        exit 1
    fi
elif [ -z "${UNINSTALL_SOURCE_URL:-}" ]; then
    printf '%s\n' 'A checksum was required but unavailable; refusing to install.' >&2
    exit 1
fi

chmod 755 "$tmp_file"
if ! actual_version=$("$tmp_file" --version 2>/dev/null); then
    printf '%s\n' 'Downloaded file failed its self-check; refusing to install.' >&2
    exit 1
fi
if [ "$actual_version" != "uninstall $RELEASE_VERSION" ]; then
    printf '%s\n' "Downloaded version mismatch (expected $RELEASE_VERSION); refusing to install." >&2
    exit 1
fi

destination_dir=$(dirname "$DESTINATION")
privilege_helper=
if [ ! -d "$destination_dir" ]; then
    if mkdir -p "$destination_dir" 2>/dev/null; then
        :
    elif command -v sudo >/dev/null 2>&1; then
        privilege_helper=sudo
        sudo install -d -m 755 "$destination_dir"
    elif command -v doas >/dev/null 2>&1; then
        privilege_helper=doas
        doas install -d -m 755 "$destination_dir"
    elif command -v pkexec >/dev/null 2>&1; then
        privilege_helper=pkexec
        pkexec install -d -m 755 "$destination_dir"
    else
        printf '%s\n' "Cannot create $destination_dir; set a writable PREFIX or install sudo/doas/pkexec." >&2
        exit 1
    fi
fi

if [ -w "$destination_dir" ]; then
    stage_file=$(mktemp "$destination_dir/.uninstall.XXXXXX")
    install -m 755 "$tmp_file" "$stage_file"
    "$stage_file" --version >/dev/null
    mv -f "$stage_file" "$DESTINATION"
    stage_file=
else
    if [ -z "$privilege_helper" ]; then
        if command -v sudo >/dev/null 2>&1; then privilege_helper=sudo
        elif command -v doas >/dev/null 2>&1; then privilege_helper=doas
        elif command -v pkexec >/dev/null 2>&1; then privilege_helper=pkexec
        else
            printf '%s\n' "Cannot write to $destination_dir; set a writable PREFIX or install sudo/doas/pkexec." >&2
            exit 1
        fi
    fi
    random_name=$(basename "$tmp_file")
    stage_file="$destination_dir/.${random_name}.new"
    "$privilege_helper" install -m 755 "$tmp_file" "$stage_file"
    "$stage_file" --version >/dev/null
    "$privilege_helper" mv -f "$stage_file" "$DESTINATION"
    stage_file=
fi

printf '%s\n' "Installed uninstall to $DESTINATION"
printf '%s\n' 'Try: uninstall FreeCAD'
