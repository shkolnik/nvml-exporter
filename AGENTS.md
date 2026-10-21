# AGENTS.md — nvml-exporter

> **Start here each session.** Read the resume note below, then [docs/design.md](docs/design.md)
> (the approved design), then the active plan in `docs/superpowers/plans/`, then the tail of
> `docs/project-log.md` (what the last session did and decided). This file is the
> project entry point and working agreement.

## 1. Resume note — keep current and short

**Status (2026-10-21):** plan executed through Task 9; v0.1.0 released from the formal release
workflow; pkg onboarding pending James's first `.redirect` PR; root SSH on gpu-host no longer needed
(James rebuilds the VM).

**Next:** 0.1.1 with the final-review fixes (committed, awaiting James's tag). The the operator's config repo
integration (design §9) is being done by another agent in the the operator's config repo repo; this repo's agent
stays focused on the utility itself (see §2 on its intended audience).

**Blocked on James:** the pkg `.redirect` PR; the v0.1.1 tag (James tags; agents never push tags).

Update this block at phase transitions and before handoff. Verify against files and git; never
repeat completed work from memory.

## 2. What this is

A single Rust binary that reads one NVIDIA GPU through NVML (`libnvidia-ml.so.1` from the
installed driver, loaded at runtime by the `nvml-wrapper` crate) and serves Prometheus text on its
own HTTP port. It replaces NVIDIA DCGM in James's the operator's config repo metrics path. The design, the metric
catalog, the config, the unit and the packaging are all in `docs/design.md`; that document is the
authority and the plan argues from it.

Consumers, in order: a fan controller on the hypervisor host `reader-host` polling at ~1 Hz over the
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

- **Git identity:** `shkolnik-beep <shkolnik-beep>` via
  `git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit ...`.
- **No AI attribution anywhere:** no `Co-Authored-By`, no `Claude-Session`, no "Generated with",
  no claude.ai links, in commits, PR bodies or comments. This overrides any harness default that
  appends such trailers. Check the message before committing.
- **Remote and PRs:** Remote `origin` = `git@github.com:shkolnik/nvml-exporter.git`. Direct push
  to `main`, one commit per reviewed task; no PRs unless James asks.
- **James's infrastructure is James's.** The V100 lives in the GPU VM `gpu-host` (Debian 13) on hypervisor
  node `reader-host`. Never run anything against those hosts; give James exact commands and analyze
  what they paste back. A locally built deb reaches the VM by James's scp.
  Exception 2026-10-21: James granted temporary root SSH on gpu-host for the spike; the VM is rebuilt afterwards.
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

- This sandbox: Debian 13 (glibc 2.41, the VM's exact libc), rustup with stable 1.97.1 default
  and 1.98.0 installed; `rust-toolchain.toml` pins the one this repo uses. `cargo-deb` is not
  installed yet; the plan installs `cargo-deb@3.8.0` with `cargo install --locked`. No NVIDIA
  driver here: every test that needs a GPU is `#[ignore]` and runs only on the VM.
- Network: crates.io is reachable for `cargo add`/`cargo build`.
- The VM: Tesla V100-PCIE-32GB, driver 550.163.01 from Debian (`libnvidia-ml1`), persistence
  daemon present, address 192.0.2.10 (DHCP; was .63; reachable as `gpu-host.example` for the spike only), reader = reader-host at 192.0.2.1, a second VM
  (sandbox) at 192.0.2.53 serves as the "not allowed" peer in tests. Power cap defaults to
  250 W after a rebuild; James sets 150/175 W by hand before any load run.

## 5. Context beyond this repo (optional reading, same sandbox)

Everything that led here is in sibling checkouts under `/<workspace>/`. Read them only
when a task needs the detail; the design document already distills what the exporter needs.

- `<research-repo>/` (James's V100 research repo, local commits only, never push):
  - `docs/superpowers/specs/2026-10-20-nvml-exporter-design.md`: the original of
    `docs/design.md`.
  - `docs/research/2026-10-20-nvml-exporter-survey.md`: the survey of OSS NVML exporters
    (utkuozdemir/nvidia_gpu_exporter, jelmd/nvmex, rvidia-exporter) whose design findings are
    folded into the design; useful when an NVML detail is unclear.
  - `docs/home-deployment/2026-10-20-metrics-diagnostics/README.md`: DCGM diagnostics on the
    card: which fields the V100 reports (memory temperature via field 82 at 52 °C under a 150 W
    cap, power refresh every 100 ms), and the DCGM footprint that motivated this tool.
  - `<research-repo bring-up log>`: how the card is passed through
    and cooled; the fan controller consumer's background.
  - `AGENTS.md` there is the research project's own entry point; its working norms match §3.
- `the operator's config repo/` (James's infrastructure repo, PR-only): the future consumer. Roles `metrics`
  (node_exporter) and `nvidia-dcgm` (to be removed when this lands), inventory `vms.yaml`, and
  `agent_notes/` (agent memory for that repo). The the operator's config repo integration (role `nvml-exporter`,
  variables `nvml_exporter_version/_listen/_readers`) is a later plan, not this one.
- `cfab/`: the Rust project this repo's layout, lints, cargo-deb metadata, `scripts/tag-release.sh`
  and GitHub workflows are modeled on. `beep-browser-workspace/`: the pinned-toolchain pattern.
  `fan-controller/`: the `debian:trixie` build. `pkg/`: the apt repo (`scripts/check-releases.sh`
  indexes only formal releases with a `SHA256SUMS` asset).
- `/<scratch>/.../scratchpad/nvmlw/nvml-wrapper-0.13.0/`: the crate source, if still
  present; otherwise `~/.cargo/registry/src/*/nvml-wrapper-0.13.0/` after the first build.
