#!/bin/sh
set -eu

UNINSTALL_BIN=${UNINSTALL_BIN:-./uninstall}

if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y ed
    "$UNINSTALL_BIN" ed --backend APT --confirm 'REMOVE APT:ed'
    if dpkg-query -W ed >/dev/null 2>&1; then
        printf '%s\n' 'APT integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v dnf5 >/dev/null 2>&1 || command -v dnf >/dev/null 2>&1; then
    dnf_command=dnf
    if ! command -v dnf >/dev/null 2>&1; then dnf_command=dnf5; fi
    "$dnf_command" install -y ed
    identifier=$(rpm -q --qf '%{NAME}.%{ARCH}' ed)
    "$UNINSTALL_BIN" "$identifier" --backend DNF --confirm "REMOVE DNF:$identifier"
    if rpm -q ed >/dev/null 2>&1; then
        printf '%s\n' 'DNF integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v apk >/dev/null 2>&1; then
    apk add ed
    "$UNINSTALL_BIN" ed --backend APK --confirm 'REMOVE APK:ed'
    if apk info --exists ed; then
        printf '%s\n' 'APK integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm ed
    "$UNINSTALL_BIN" ed --backend Pacman --confirm 'REMOVE Pacman:ed'
    if pacman -Q ed >/dev/null 2>&1; then
        printf '%s\n' 'Pacman integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v zypper >/dev/null 2>&1; then
    zypper --non-interactive install ed
    identifier=$(rpm -q --qf '%{NAME}.%{ARCH}' ed)
    "$UNINSTALL_BIN" "$identifier" --backend Zypper --confirm "REMOVE Zypper:$identifier"
    if rpm -q ed >/dev/null 2>&1; then
        printf '%s\n' 'Zypper integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v xbps-install >/dev/null 2>&1; then
    xbps-install -Sy ed
    "$UNINSTALL_BIN" ed --backend XBPS --confirm 'REMOVE XBPS:ed'
    if xbps-query ed >/dev/null 2>&1; then
        printf '%s\n' 'XBPS integration removal left ed installed.' >&2
        exit 1
    fi
else
    printf '%s\n' 'No exact-removal integration scenario for this image; skipped.'
fi
