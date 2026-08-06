#!/bin/sh
set -eu

RELEASE_VERSION=0.20.0
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

for required_command in chmod curl dirname install mkdir mktemp mv rm sed tr uname; do
    if ! command -v "$required_command" >/dev/null 2>&1; then
        printf '%s\n' "$required_command is required but was not found." >&2
        exit 1
    fi
done

case "$(uname -m)" in
    x86_64|amd64) architecture=x86_64 ;;
    aarch64|arm64) architecture=aarch64 ;;
    armv7l|armv7) architecture=armv7 ;;
    i386|i486|i586|i686) architecture=i686 ;;
    *) printf '%s\n' "No uninstall release is available for $(uname -m)." >&2; exit 1 ;;
esac

asset="uninstall-linux-${architecture}-musl"
default_url="https://github.com/JaredTweed/uninstall/releases/download/v${RELEASE_VERSION}/${asset}"
source_url=${UNINSTALL_SOURCE_URL:-$default_url}
checksum_url=${UNINSTALL_CHECKSUM_URL:-"${source_url}.sha256"}
destination="$PREFIX/bin/uninstall"

temporary=$(mktemp "${TMPDIR:-/tmp}/uninstall.XXXXXX")
checksum_file=$(mktemp "${TMPDIR:-/tmp}/uninstall-checksum.XXXXXX")
stage_file=
privilege_helper=
cleanup() {
    rm -f "$temporary" "$checksum_file"
    if [ -n "$stage_file" ]; then
        if [ -n "$privilege_helper" ]; then
            "$privilege_helper" rm -f -- "$stage_file" 2>/dev/null || true
        else
            rm -f -- "$stage_file" 2>/dev/null || true
        fi
    fi
}
trap cleanup EXIT HUP INT TERM

printf '%s\n' "Downloading uninstall ${RELEASE_VERSION} for ${architecture}..."
curl -fsSL "$source_url" -o "$temporary"

expected_checksum=${UNINSTALL_SHA256:-}
if [ -z "$expected_checksum" ]; then
    if ! curl -fsSL "$checksum_url" -o "$checksum_file"; then
        printf '%s\n' 'A published SHA-256 checksum is required; refusing to install.' >&2
        exit 1
    fi
    expected_checksum=$(sed -n '1{s/[[:space:]].*//;p;}' "$checksum_file")
fi
case "$expected_checksum" in
    *[!0-9A-Fa-f]*|'') printf '%s\n' 'The expected SHA-256 checksum is invalid.' >&2; exit 1 ;;
esac
if [ "${#expected_checksum}" -ne 64 ]; then
    printf '%s\n' 'The expected SHA-256 checksum is invalid.' >&2
    exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
    actual_checksum=$(sha256sum "$temporary" | sed 's/[[:space:]].*//')
elif command -v shasum >/dev/null 2>&1; then
    actual_checksum=$(shasum -a 256 "$temporary" | sed 's/[[:space:]].*//')
elif command -v openssl >/dev/null 2>&1; then
    actual_checksum=$(openssl dgst -sha256 "$temporary" | sed 's/^.*= //')
else
    printf '%s\n' 'sha256sum, shasum, or openssl is required to verify the download.' >&2
    exit 1
fi
actual_checksum=$(printf '%s' "$actual_checksum" | tr 'A-F' 'a-f')
expected_checksum=$(printf '%s' "$expected_checksum" | tr 'A-F' 'a-f')
if [ "$actual_checksum" != "$expected_checksum" ]; then
    printf '%s\n' 'Downloaded checksum mismatch; refusing to install.' >&2
    exit 1
fi

chmod 755 "$temporary"
if ! actual_version=$("$temporary" --version 2>/dev/null); then
    printf '%s\n' 'Downloaded executable failed its self-check; refusing to install.' >&2
    exit 1
fi
if [ "$actual_version" != "uninstall $RELEASE_VERSION" ]; then
    printf '%s\n' "Downloaded version mismatch (expected $RELEASE_VERSION); refusing to install." >&2
    exit 1
fi

destination_dir=$(dirname "$destination")
if [ ! -d "$destination_dir" ]; then
    if mkdir -p "$destination_dir" 2>/dev/null; then
        :
    elif command -v sudo >/dev/null 2>&1; then privilege_helper=sudo; sudo install -d -m 755 "$destination_dir"
    elif command -v doas >/dev/null 2>&1; then privilege_helper=doas; doas install -d -m 755 "$destination_dir"
    elif command -v pkexec >/dev/null 2>&1; then privilege_helper=pkexec; pkexec install -d -m 755 "$destination_dir"
    else
        printf '%s\n' "Cannot create $destination_dir; set a writable PREFIX or install sudo, doas, or pkexec." >&2
        exit 1
    fi
fi

if [ -w "$destination_dir" ]; then
    stage_file=$(mktemp "$destination_dir/.uninstall.XXXXXX")
    install -m 755 "$temporary" "$stage_file"
    "$stage_file" --version >/dev/null
    mv -f "$stage_file" "$destination"
    stage_file=
else
    if [ -z "$privilege_helper" ]; then
        if command -v sudo >/dev/null 2>&1; then privilege_helper=sudo
        elif command -v doas >/dev/null 2>&1; then privilege_helper=doas
        elif command -v pkexec >/dev/null 2>&1; then privilege_helper=pkexec
        else
            printf '%s\n' "Cannot write to $destination_dir; set a writable PREFIX or install sudo, doas, or pkexec." >&2
            exit 1
        fi
    fi
    stage_file=$("$privilege_helper" mktemp "$destination_dir/.uninstall.XXXXXX")
    "$privilege_helper" install -m 755 "$temporary" "$stage_file"
    "$stage_file" --version >/dev/null
    "$privilege_helper" mv -f "$stage_file" "$destination"
    stage_file=
fi

printf '%s\n' "Installed uninstall to $destination"
printf '%s\n' 'Try: uninstall FreeCAD'
