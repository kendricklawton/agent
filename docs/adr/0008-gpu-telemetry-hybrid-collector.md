# 0008 — GPU telemetry: DCGM/NVML values + ioctl attribution

- **Status:** Accepted
- **Date:** 2026-06-23
- **Deciders:** K-Henry
- **Milestone:** M4

## Context
GPU/AI-workload awareness is the project's wedge, but GPUs live mostly in **userspace/driver**, not
the kernel — eBPF can't read SM utilization directly. Two hard facts shape the design:
- The trustworthy metric *values* (utilization, memory, SM occupancy, per-process usage) come from
  **NVML/DCGM**, not from anything kernel-visible.
- Production serving images (vLLM, TGI, TensorRT-LLM) frequently **static-link CUDA** into a fat
  `.so`, so `uprobe`s on `libcudart` (`cudaLaunchKernel`/`cudaMalloc`) have nothing to attach to and
  **silently no-op** — they fail on a large share of real workloads.

We also need the pipeline to be testable without GPU hardware.

## Decision
We will use a **hybrid collector** behind one pluggable interface, **NVIDIA-first** but
vendor-neutral by design:
- **Values (source of truth): NVML/DCGM** (`nvml-wrapper`; model after `dcgm-exporter`).
  `nvmlDeviceGetComputeRunningProcesses` gives the per-PID list + each process's **GPU memory**;
  per-process **utilization** comes from `nvmlDeviceGetProcessUtilizationSamples` or DCGM. PIDs are
  joined to pods via enrichment.
- **Attribution (the eBPF wedge): trace the `/dev/nvidia*` + `/dev/nvidia-uvm` ioctl boundary** for
  *which pod is on the GPU* — immune to static linking and stripped binaries. `libcudart` uprobes
  remain an opportunistic extra where symbols exist.
- **Inference KPIs:** scrape the serving runtime's Prometheus endpoint (vLLM: tokens/sec, queue
  depth, KV-cache) rather than fragile framework uprobes.
- **A mock/synthetic collector** so the pipeline and rules (M5) are fully testable **without GPU
  hardware**; real-GPU validation is a manual test on an actual NVIDIA + Linux node.

## Consequences
- Metric values are trustworthy (DCGM) *and* per-pod attribution survives static linking (ioctl) —
  neither alone suffices.
- The **NVIDIA ioctl ABI is proprietary and drifts across driver versions** → treat the ioctl signal
  as *activity/attribution*, not metric values; version-gate the decoded numbers.
- Needs driver + device access (privileged or device-plugin); DCGM implies an out-of-process NVIDIA
  daemon on GPU nodes (see [ADR-0004](0004-single-self-contained-binary.md)).
- AMD/ROCm is deferred; the collector interface stays vendor-neutral so it can be added later.

## Alternatives considered
- **`libcudart` uprobes as the primary signal** — rejected: no-ops on static-linked/stripped
  production images.
- **Pure-eBPF GPU metrics** — rejected: GPU utilization simply isn't kernel-visible.
- **DCGM/NVML only (no ioctl)** — rejected: gives values but no kernel-level per-pod attribution —
  loses the eBPF differentiator.
