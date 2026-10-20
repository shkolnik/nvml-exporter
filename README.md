# nvml-exporter

A small Prometheus exporter for NVIDIA GPUs that reads NVML (`libnvidia-ml.so.1`, shipped with
the driver) directly and serves the result on its own HTTP port. One process, one thread, no
runtime beyond libc and the driver's NVML; read-only by construction.

Status: in development; see `AGENTS.md` for the working agreement and `docs/design.md` for the
design.

License: MIT OR Apache-2.0.
