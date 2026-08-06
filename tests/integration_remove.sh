#!/bin/sh
set -eu

if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y ed
    ./uninstall ed --backend APT --confirm 'REMOVE APT:ed'
    if dpkg-query -W ed >/dev/null 2>&1; then
        printf '%s\n' 'APT integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v dnf >/dev/null 2>&1; then
    dnf install -y ed
    identifier=$(rpm -q --qf '%{NAME}.%{ARCH}' ed)
    ./uninstall "$identifier" --backend DNF --confirm "REMOVE DNF:$identifier"
    if rpm -q ed >/dev/null 2>&1; then
        printf '%s\n' 'DNF integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v apk >/dev/null 2>&1; then
    apk add ed
    ./uninstall ed --backend APK --confirm 'REMOVE APK:ed'
    if apk info --exists ed; then
        printf '%s\n' 'APK integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm ed
    ./uninstall ed --backend Pacman --confirm 'REMOVE Pacman:ed'
    if pacman -Q ed >/dev/null 2>&1; then
        printf '%s\n' 'Pacman integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v zypper >/dev/null 2>&1; then
    zypper --non-interactive install ed
    identifier=$(rpm -q --qf '%{NAME}.%{ARCH}' ed)
    # Zypper has no reliable machine-readable removal set. Exercise the
    # deliberately interactive preview path and Zypper's own prompt.
    printf '%s\n' 'y' 'y' | ./uninstall "$identifier"
    if rpm -q ed >/dev/null 2>&1; then
        printf '%s\n' 'Zypper integration removal left ed installed.' >&2
        exit 1
    fi
elif command -v xbps-install >/dev/null 2>&1; then
    xbps-install -Sy ed
    ./uninstall ed --backend XBPS --confirm 'REMOVE XBPS:ed'
    if xbps-query ed >/dev/null 2>&1; then
        printf '%s\n' 'XBPS integration removal left ed installed.' >&2
        exit 1
    fi
else
    printf '%s\n' 'No exact-removal integration scenario for this image; skipped.'
fi
