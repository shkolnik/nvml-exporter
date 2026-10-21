# Project log

Append-only. Newest entry last. Format and rules in `AGENTS.md` §3.

## 2026-10-21T00:28Z — Task 1 complete: crate scaffold, pins, lints, setter ban
Commit: see this commit. Tests: 2 passed. Notes: toolchain pinned to 1.98.0; nvml-wrapper resolved to 0.13.0; the 33-name setter list matches device.rs exactly. Clippy group lints use priority -1 so the individual allows apply.

## 2026-10-21T00:31Z — Task 2 complete: catalog and exposition
Commit: see this commit. Tests: 8 passed (4 expo, 2 catalog, 2 existing). Notes: CATALOG holds 23 entries in §4.4 order; 19 help strings written from §4.4's meaning/note columns. render() panics on a name not in CATALOG and emits HELP/TYPE only for metrics with samples. docs/metrics.md is generated (UPDATE_DOCS=1 cargo test --test catalog).

## 2026-10-21T00:33Z — Task 3 complete: config and allow-list
Commit: see this commit. Tests: 5 passed (tests/config.rs). Notes: bare reader addresses become /32 or /128 so entries() Displays 192.0.2.1/32; allows() canonicalizes the peer so ::ffff:a.b.c.d matches IPv4 entries and loopback; hostnames in readers are rejected. ConfigError::Io carries the path.
