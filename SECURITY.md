# Security policy

`uninstall` treats package metadata, desktop files, command output, paths, and
environment variables as untrusted input. It does not run a shell, does not run
the whole program as root, and never deletes discovered user data without a
separate opt-in selection.

Please report a suspected command-injection, privilege-boundary, path traversal,
symlink-race, preview, protection, or unintended-deletion issue privately through
GitHub's security-advisory interface. Include the distribution, package-manager
and uninstall versions, installation scope, and sanitized terminal output. Do not
include credentials, private repository URLs, or unrelated package history.

Security fixes are supported on the newest tagged release. Older releases may
receive a notice but are not maintained as separate branches.
