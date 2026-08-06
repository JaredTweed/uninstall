# Threat model

The primary safety boundary is between read-only discovery and a small number of
explicitly displayed destructive commands. Package names, versions, repository
metadata, manager output, desktop entries, environment variables, and filesystem
names may all be malicious or malformed.

The program therefore uses argument arrays without a shell, neutralizes terminal
control characters, pins the selected executable before crossing a privilege
boundary, previews native transactions when possible, fingerprints the preview,
and repeats it immediately before execution. A changed transaction is aborted.

Detected data paths are optional. Broad XDG roots are rejected. Selected paths
are held open and snapshotted by device, inode, type, change time, and parent
identity, then revalidated and removed relative to the held parent directory.
Holding the descriptors prevents an unlinked inode from being recycled as a
replacement while the user confirms. A failed uninstall keeps all selected data.

Package-manager simulations remain fallible: installed databases may change,
history may be pruned, conditional dependencies may be ambiguous, and immutable
systems may activate changes in a later deployment. Unknown or unsupported
impact is never represented as an exact safe preview.
