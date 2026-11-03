# Muttley

Small, local-first utilities for repetitive software engineering chores.

## Guard

`muttley guard` checks added staged lines for potential personal home-directory
paths. It recognises conventional macOS and Linux home prefixes, including when
the path belongs to someone other than the person running the command.

```sh
cargo run -- guard
```

Example finding:

```text
settings.txt:2: home-path: potential personal home-directory path
```

Findings go to standard output. Each identifies the repository-relative filename
and staged line number. Matched paths and source snippets are omitted, and control
characters in filenames are escaped. Inspection errors go to standard error;
Git's raw diagnostics are omitted because they can contain personal information.

| Exit code | Meaning |
| --- | --- |
| `0` | No unsuppressed home-path findings in added staged text. |
| `1` | One or more home-path findings. |
| `2` | Inspection failed, including missing Git or unresolved conflicts. |

### What gets checked

- The Git index, including before the first commit. An unstaged correction does
  not change what Guard sees.
- Added lines in Git text diffs across the repository, even when run from a
  subdirectory. Unchanged lines and removed lines are ignored.
- Renamed files as a removal and addition, so their contents are checked again.

Guard leaves the working copy and index unchanged. It disables external diff
helpers and text-conversion filters. It uses the installed Git executable and
does not make network requests.

### Current limits

Guard detects literal home-path shapes. Windows home paths, encoded paths,
credentials, unstaged files, history, binary content and submodule contents are
outside its scope. Symlinks are not followed; their staged target text can produce
a home-path finding.

### Intentional examples

Show a finding's suppression fingerprint:

```sh
cargo run -- guard --show-fingerprints
```

For each deliberate example, copy its complete `home-path:` fingerprint into a
`.muttleyignore` file at the repository root, followed by `#` and a reason. Use
one entry per line. Blank lines and full-line comments are allowed.

```text
# Deliberate documentation examples.
home-path:<copy the complete 64-character fingerprint here> # Explain why this example is safe.
```

The placeholder above is explanatory and is not a valid suppression entry.
Fingerprints cover the repository-relative filename, staged line number and
complete line content. Moving or editing the line requires reviewing and updating
the suppression. Other files and other lines remain checked.

Stage `.muttleyignore` along with the example. Guard reads only its staged
contents, including unchanged tracked entries. An unstaged edit cannot add or
remove a suppression. The file must be regular UTF-8 text; malformed entries and
entries without a reason cause exit code `2`. Guard never writes suppressions.

### Build and check

Requires Rust with edition 2024 support and Git on `PATH`. Tests use temporary
repositories with synthetic data.

```sh
cargo build --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Pull requests run formatting, integration tests, Clippy and release builds on
Linux x86-64, macOS Apple Silicon and macOS Intel. Each build runs the executable's
version command and uploads a compressed archive as a workflow artifact.

### GitHub releases

After merging a reviewed version, push a tag matching the version in `Cargo.toml`
(for example, `v0.1.0`). The same pipeline builds and checks all three platforms,
then publishes a GitHub Release with the executable archives, a `SHA256SUMS` file
and generated release notes. Tags that do not match the package version fail
before publication. Prerelease versions are marked as prereleases.

Download the archive for your platform from the repository's GitHub Releases
page, extract it and put `muttley` in a directory on your `PATH`. Linux binaries
use glibc from the GitHub-hosted Ubuntu build image. macOS executables are unsigned
and are not notarised.
