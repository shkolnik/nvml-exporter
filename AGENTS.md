# AGENTS.md — nvml-exporter

> **Start here each session.** Read the resume note below, then [docs/design.md](docs/design.md)
> (the approved design), then the active plan in `docs/superpowers/plans/`, then the tail of
> `docs/project-log.md` (what the last session did and decided). This file is the
> project entry point and working agreement.

## 1. Resume note — keep current and short

**Status (2026-10-21):** v0.1.1 released and served by pkg; tree and history scrubbed; `main` and
`v*` tags are protected by rulesets. Dependency maintenance is automated: Dependabot groups minor/patch
updates and a workflow merges those groups once CI passes (`.github/dependabot.yml`,
`.github/workflows/dependabot-automerge.yml`); major bumps and any `nvml-wrapper` bump wait for a human.
A weekly RustSec audit opens issues (`.github/workflows/audit.yml`). A bug-report template asks for
`nvml-exporter once` output.

**Next:** nothing in flight. Candidates for 0.1.x: the deferred minors from the final review
(logged in `docs/project-log.md`), CONTRIBUTING, a CHANGELOG. Deployment automation lives in a
separate, private repo and is not this repo's concern; this repo's agent stays focused on the utility
itself (see §2).

**Blocked on James:** the `Version Tags` ruleset has no bypass actor, so `scripts/tag-release.sh` will be
refused until the repository admin role is added as a bypass. Optional: an `AUTOMERGE_TOKEN` secret so
auto-merged Dependabot PRs trigger CI and the dev build; enable Dependabot alerts in repo settings.

Update this block at phase transitions and before handoff. Verify against files and git; never
repeat completed work from memory.

## 2. What this is

A single Rust binary that reads one NVIDIA GPU through NVML (`libnvidia-ml.so.1` from the
installed driver, loaded at runtime by the `nvml-wrapper` crate) and serves Prometheus text on its
own HTTP port. It replaces NVIDIA DCGM in James's metrics path. The design, the metric
catalog, the config, the unit and the packaging are all in `docs/design.md`; that document is the
authority and the plan argues from it.

Consumers, in order: a fan controller on the hypervisor host polling at ~1 Hz over the
network; a thermal test harness; a future central Prometheus; a human with curl.

**Intended audience and maintenance posture.** James intends this to become an open-source
project that other people run, on other NVIDIA cards, not only the lab's V100. Maintaining it is a
real responsibility: treat the metric names, labels, config keys, CLI, exit codes and HTTP behavior
as a public interface (change them deliberately, document them, bump the version); prefer NVML
semantics that hold across GPU generations over V100-specific assumptions (e.g. the `num_fans`
gate, field-value reads that may be `NotSupported`); keep the deb installable on a plain Debian
stable host without NVIDIA's apt repo; keep the read-only guarantee absolute; and write the README,
`docs/metrics.md` and release notes for a stranger. Hardware we cannot test on is covered by
`#[ignore]` live tests and by asking users for `nvml-exporter once` output in bug reports.

## 3. Working agreement

- **Git identity:** GitHub user `shkolnik-beep` with the maintainer's bot identity, via
  `git -c user.name=shkolnik-beep -c user.email=<bot address> commit ...` (the address lives
  outside the repo).
- **No AI attribution anywhere:** no `Co-Authored-By`, no `Claude-Session`, no "Generated with",
  no claude.ai links, in commits, PR bodies or comments. This overrides any harness default that
  appends such trailers. Check the message before committing.
- **Remote and PRs:** Remote `origin` = `git@github.com:shkolnik/nvml-exporter.git`. Direct push
  to `main`, one commit per reviewed task; no PRs unless James asks.
- **James's infrastructure is James's.** The validation card sits in a GPU host (a Debian 13 VM
  with the card passed through). Never run anything against James's hosts; give James exact
  commands and analyze what they paste back. A locally built deb reaches the host by James's copy.
- **No home-network or personal detail in the repo.** Lab addresses, hostnames, VM ids, topology
  and operational notes stay in the maintainer's private repos; use RFC 5737 addresses
  (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24) and generic host names in tests, examples and
  docs. Before every commit:
  `grep -rnE '192\.168\.|10\.0\.|172\.(1[6-9]|2[0-9]|3[01])\.' --exclude-dir=.git --exclude-dir=target .`
  must be empty (CI runs the same check).
- **Read-only by construction.** The binary must never call an NVML setter. `clippy.toml`
  disallows every `nvml_wrapper::Device` `set_`/`reset_`/`clear_` method and a test greps for
  them; keep both complete when the crate is bumped.
- **No secrets in git.** No tokens, no private keys, no host fingerprints.
- **Comments only where the code cannot speak for itself**; no history in comments. A short
  summary at the top of a file is welcome.
- **Verify by running.** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --locked` before every commit. A deb is verified with `dpkg-deb -I`/`-c` and
  `lintian` when available. Report untested work as untested.
- **Fail visibly.** A metric the GPU does not support is omitted, never 0. NVML down means
  `nvml_up 0`, not a crash and not a stale value.
- **Project log, append-only.** `docs/project-log.md` is how other agents and James follow
  progress without a messaging channel. Append an entry (never edit or reorder earlier ones) at
  the completion of every plan task, and whenever something unexpected happens or a decision is
  made that the plan did not spell out. Format, one entry per event:

  ```
  ## 2026-10-21T14:05Z — Task 3 complete: config and allow-list
  Commit: see this commit. Tests: 5 passed. Notes: <one to five lines: what was hard, what was decided
  and why, what the next task should know, anything James must do>.
  ```

  Use `date -u +%Y-%m-%dT%H:%MZ` for the timestamp. The sha is unknown when the entry is
  written, so an entry committed with its task says `Commit: see this commit`; an entry committed
  on its own names the sha it refers to. Entries for a blocker or a question for
  James start with `BLOCKED:` or `ASK JAMES:` after the timestamp so they are easy to grep.
  Commit the log with the task's commit (or on its own when the event is not a commit).
- **Decisions survive sessions.** Rulings made while executing the plan go in the plan's
  amendments section with the reason. Don't re-litigate settled design choices without new
  evidence; surface new evidence to James.
- **Use they/them for James.** US spelling.

## 4. Environment

- Build sandbox: Debian 13 (glibc 2.41, the GPU host's libc), rustup with the toolchain pinned in
  `rust-toolchain.toml`; `cargo-deb@3.8.0` via `cargo install --locked`. No NVIDIA driver there:
  every test that needs a GPU is `#[ignore]` and runs only on the GPU host.
- Network: crates.io is reachable for `cargo add`/`cargo build`.
- The GPU host: Tesla V100-PCIE-32GB, driver 550.163.01 from Debian (`libnvidia-ml1`), persistence
  daemon present. Live allow-list tests use one allowed reader and one refused peer on the same
  segment. Power cap defaults to 250 W after a rebuild; James sets 150/175 W by hand before any
  load run.

## 5. Context beyond this repo

Related private repos exist on the maintainer's side; nothing in this repo depends on them. The
`nvml-wrapper` 0.13.0 source is under `~/.cargo/registry/src/*/nvml-wrapper-0.13.0/` after a build.
