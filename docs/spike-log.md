# Spike log: nvml-exporter on the V100

Date: 2026-10-21 (VM clock Oct 20 19:50 local time). Plan Task 6; expectations from `docs/design.md` §8.

Deb: `nvml-exporter_0.1.0-1_amd64.deb`, sha256
`2beeb6d4051d05d8d9a30a4296f920cdc48bc7277fbc9f594e9867ca5262bd24`. `dpkg -i` clean; postinst
created the `multi-user.target.wants` symlink.

How it was run: by the agent over temporary root SSH on the GPU VM `gpu-host`, a one-time exception to
AGENTS.md §3 authorized by James for this spike; the VM is rebuilt afterwards.

Host: Debian 13.7, eth0 192.0.2.10/24 (DHCP; lease moved from .63), Tesla V100-PCIE-32GB
UUID GPU-00000000-0000-0000-0000-000000000000, `libnvidia-ml1` 550.163.01-2, nvidia-persistenced
active. `/dev/nvidia0` and `/dev/nvidiactl` are `crw-rw-rw- root:root` (195,0 / 195,255); group
`video` = gid 44.

## 1. `nvml-exporter once` versus `nvidia-smi -q`

Commands: `nvml-exporter once; echo "exit $?"`, `nvidia-smi -q` (temperature, power, clocks),
`sleep 2; nvml-exporter once | grep -E 'nvml_energy|nvml_power_watts'`. Exit 0.

| field | exporter | nvidia-smi |
|---|---|---|
| `nvml_up` / `nvml_device_count` | 1 / 1 | – |
| driver version | 550.163.01 | – |
| `nvml_gpu_info` | name "Tesla V100-PCIE-32GB", pci_bus_id 00000000:00:00.0, vbios 88.00.00.00.00 | – |
| GPU temperature | 36 | GPU Current Temp 36 C |
| memory temperature (field 82) | 38 | Memory Current Temp 38 C |
| `nvml_power_watts` | 27.326 (2 s later 27.427) | Instantaneous Power Draw 27.33 W |
| `nvml_power_limit_watts` | 250 | Current Power Limit 250.00 W |
| `nvml_energy_joules_total` | 7859.603 → 7915.19 after 2 s (~27.8 W average) | – |
| SM / memory clock (Hz) | 135000000 / 877000000 | SM 135 MHz, Memory 877 MHz |
| utilization gpu / memory | 0 / 0 | – |
| memory used / total (bytes) | 65536 / 34359738368 | – |
| `nvml_throttle_reason` | gpu_idle 1, other eight 0 | – |
| PCIe throughput | not collected (`collect_pcie = false`) | – |
| `nvml_fan_speed_ratio` | omitted, but `nvml_exporter_scrape_errors_total{metric="nvml_fan_speed_ratio"} 1`; debug: `a supplied argument was invalid` | no fan |
| `nvml_persistence_mode` | 1 | – |
| `nvml_exporter_scrape_duration_seconds` | 0.015 (first scrape, includes `Nvml::init`) | – |

Log at info: one line `NVML up: driver 550.163.01, devices ["Tesla V100-PCIE-32GB"]`.

Conclusion: every §8.1 expectation met (temperatures exact, power within 0.01 W, energy rising,
gpu_idle 1). `Clock::SM` equals nvidia-smi "SM". The fan error count is a bug: `fan_speed(0)`
returns `NvmlError::InvalidArgument` on the fanless card, not `NotSupported` (fixed, see Decisions).

## 2. Service under the hardened unit

`systemctl status nvml-exporter`: enabled, `activating (auto-restart)`, status=2 every 5 s; the
journal repeats `serve: not implemented until the HTTP server lands` with a climbing restart
counter; memory peak 1.9M. The expected stub loop: install → enable → start works. Stopped with
`systemctl stop nvml-exporter`; the Task 7 upgrade starts it again.

Hardening: `systemd-run --wait --pipe` with every `[Service]` property of the §6 unit
(DynamicUser=yes, SupplementaryGroups=video, the three DeviceAllow lines, DevicePolicy=closed,
ProtectSystem=strict, ProtectHome, PrivateTmp, NoNewPrivileges, ProtectKernelTunables/Modules/
ControlGroups, RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX, RestrictNamespaces,
LockPersonality, MemoryDenyWriteExecute=yes, SystemCallArchitectures=native, empty
CapabilityBoundingSet/AmbientCapabilities) running `/usr/bin/nvml-exporter once`: exit 0,
`nvml_up 1`, `nvml_memory_temperature_celsius{gpu="0"} 38`; CPU 24 ms; memory peak 14.7M.
Without MemoryDenyWriteExecute: 14.5M, same result.

Conclusion: no fallback needed; the unit ships as written. DynamicUser works because the device
nodes are world-rw and DeviceAllow admits them; SupplementaryGroups=video is harmless.

## 3. Footprint

`ls -l /usr/bin/nvml-exporter`: 1,582,632 bytes (stripped). `ldd`: linux-vdso, libgcc_s.so.1,
libc.so.6, ld-linux only; libnvidia-ml is dlopened. `time nvml-exporter once`: real 0.018 s.
`/usr/bin/time` is not installed on the VM; peak RSS comes from systemd-run accounting (14.7 MB,
whole process including libnvidia-ml).

Conclusion: meets "a few MB, well under 100 ms" for time; RSS is dominated by libnvidia-ml.

## 4. Error paths (Review Focus 5)

Not exercisable safely: the driver cannot be unloaded under passthrough without taking the VM
down, and restarting nvidia-persistenced does not invalidate handles. Not attempted. The two
placeholders in `tests/live.rs` stay as documentation.

## Decisions

- Unit unchanged; `docs/design.md` §6 records the verification.
- `Clock::SM` confirmed to match nvidia-smi "SM"; no catalog change.
- Fan: the read is gated on `Device::num_fans()` (errors there counted under
  `nvml_fan_speed_ratio`); 0 fans emits and counts nothing. `tests/live.rs` asserts no fan scrape
  error. To be confirmed on the card in Task 7's spike round.
- No load generated: the fan is hard-coded to 10–15 % duty and James must approve any load. The
  §8.1 `sw_power_cap 1` under load check is deferred; gpu_idle 1 verified the bitmask path.
- VM address is now 192.0.2.10 (DHCP); AGENTS.md §4 updated.
- Allow-list test from reader-host (192.0.2.1) and the sandbox VM (192.0.2.53), plus the `ss`
  listener check, deferred to Task 7 step 6 (needs `serve`).
