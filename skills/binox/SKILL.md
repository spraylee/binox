---
name: binox
description: Use when running GitHub-hosted CLI tools without pre-installing them (npx-style for GitHub Releases). Fetches the right prebuilt binary for the current platform and executes it — no Node.js, no cargo, no compiling. Covers binox owner/repo execution, version pinning, offline/cache mode, and the asset-name matching matrix.
---

# binox — run any GitHub tool in one line

**One-line install** (Linux/macOS; Windows: `scripts/bootstrap.ps1` from the repo):

```sh
curl -fsSL https://github.com/spraylee/binox/releases/latest/download/bootstrap.sh | sh
```

Installs to `~/.local/bin/binox` (single ~6MB binary, zero runtime deps).

## What it does

Given `owner/repo`, binox resolves the latest GitHub Release, picks the asset
matching your platform, downloads + SHA256-verifies it into a shared cache,
then `exec`s the binary. Re-runs are cache hits with **0 downloads**.

```sh
binox sharkdp/bat --version          # run latest, args pass through
binox BurntSushi/ripgrep -- --version
binox owner/repo@v1.2.3 [args...]    # pin a version, skip latest probe
binox --offline owner/repo [args...] # cache only, no network
binox --bin <name> owner/repo        # pick binary in multi-bin releases
binox --asset-template '{name}-v{version}-{target}.tar.gz' owner/repo
binox update                         # foreground self-update
```

Own flags go **before** the `owner/repo` target; everything after the target
is passed through verbatim.

## Key behaviors (the parts agents get wrong)

- **Freshness**: unpinned runs probe `releases/latest` via a 302 (3s timeout,
  no API quota). Tag unchanged → cache hit; changed → download the new one.
  `@tag` / `--offline` never probe.
- **Self-update**: background, silent, atomic (download → SHA256 → rename →
  smoke test, rollback on failure). Opt out with `BINOX_NO_SELFUPDATE=1`.
- **Asset matching** (in order): `{name}-v{ver}-{target}.tar.gz` →
  `{name}-{ver}-{target}.tar.gz` → `{name}_{target}.tar.gz` →
  `{name}-{target}.tar.gz` → same matrix for `.zip` → musl fallback → fuzzy
  triple match. `--asset-template` overrides everything. Real filenames come
  from `releases/expanded_assets/{tag}` (no API quota).
- **Cache**: `~/.cache/binox/` (Linux), `~/Library/Caches/binox/` (macOS),
  `%LOCALAPPDATA%\binox\` (Windows). Layout `{owner}/{repo}/{tag}/{target}/`.
- Does **not** touch crates.io, does not compile source, does not shim npm
  packages. GitHub Releases only, by design.

## Always-fresh reference

This skill is a **thin pointer** — before relying on details, fetch the live
docs for the installed version:

```sh
binox spraylee/binox --help
curl -fsSL https://binox.spraylee.com/llms.txt
```

Repo: https://github.com/spraylee/binox · Site: https://binox.spraylee.com
