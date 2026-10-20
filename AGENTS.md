# AGENTS.md — nvml-exporter

> **Start here each session.** Read the resume note below, then [docs/design.md](docs/design.md)
> (the approved design), then the active plan in `docs/superpowers/plans/`. This file is the
> project entry point and working agreement.

## 1. Resume note — keep current and short

**Status (2026-10-20):** repo bootstrapped locally; design approved by James; implementation
plan written (`docs/superpowers/plans/2026-10-20-nvml-exporter.md`), not yet executed.

**Next:** execute the plan task by task (the plan header names the execution skill). The GitHub
remote `shkolnik/nvml-exporter` does not exist yet: commit locally on `main`, do not create a
remote; James creates the repo, adds the agent as collaborator, and then says to push. Tasks that
need GitHub (CI run, release workflow, pkg onboarding) are marked in the plan and wait for that.

**Blocked on James:** nothing until the spike task, which needs the V100 VM (James operates it:
give the commands, analyze pasted output).

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

## 3. Working agreement

- **Git identity:** `shkolnik-beep <shkolnik-beep>` via
  `git -c user.name=shkolnik-beep -c user.email=shkolnik-beep commit ...`.
- **No AI attribution anywhere:** no `Co-Authored-By`, no `Claude-Session`, no "Generated with",
  no claude.ai links, in commits, PR bodies or comments. This overrides any harness default that
  appends such trailers. Check the message before committing.
- **Remote and PRs:** none until James says the remote exists. After that, James decides the
  flow (likely PR-only into `main` as in their other repos); ask before the first push.
- **James's infrastructure is James's.** The V100 lives in the GPU VM `gpu-host` (Debian 13) on hypervisor
  node `reader-host`. Never run anything against those hosts; give James exact commands and analyze
  what they paste back. A locally built deb reaches the VM by James's scp.
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
  daemon present, address 192.0.2.10 (DHCP), reader = reader-host at 192.0.2.1, a second VM
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
