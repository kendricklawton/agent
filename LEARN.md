# LEARN — from a SWE degree to shipping `agent`

> The study companion to [`ROADMAP.md`](./ROADMAP.md). Read [`.rules`](./.rules) for *what* we're
> building; this file is the path from *"I can write code"* to *"I can ship a kernel-level eBPF
> security agent."*

## Read this first

**Who this is written for:** you have a software-engineering degree. You can program, you know data
structures and algorithms, you've seen processes/threads/memory in an OS course, and you can use
git. You have **not** written Rust, touched the Linux kernel, used eBPF, run Kubernetes, or
programmed a GPU. That's fine — this doc assumes exactly that and builds up from there.

**Be honest with yourself about the size of this.** `agent` sits at the intersection of four things
most degrees barely touch — **the Linux kernel, systems programming in Rust, container/Kubernetes
internals, and GPUs**. Going from a fresh degree to the signature demo (a kernel probe that catches
a shell in a pod and names it) is realistically a **~9–18 month part-time journey**. That's not
discouragement — it's the *reason this skill is valuable*. eBPF + kernel work is one of the
highest-paid, most supply-constrained skills in the industry precisely because the ramp is steep.

**The golden rule: do not try to learn it all before you build.** You will drown. Learn each layer
*just enough* to start the next, build something small, and loop back when you hit a wall. The
**Stages below are a rough order**, but the [milestone map](#just-in-time-what-to-learn-before-each-milestone)
at the end is the real plan: it tells you the *minimum* slice to learn right before each ROADMAP
milestone. **Reading without a machine in front of you doesn't stick** — every stage ends with
something to *do*.

**Scope:** this is the OSS `agent` only. Cloud-side topics (databases, distributed systems,
multi-tenancy) live with `agent-cloud`.

The stages:

| Stage | What you build the ability to do | Rough time |
|------:|----------------------------------|------------|
| **0** | Live in Linux; understand how a program talks to the kernel | 4–8 weeks |
| **1** | Write real Rust (not just pass the borrow checker) | 8–12 weeks |
| **2** | Understand kernel internals: syscalls, cgroups, namespaces | 4–8 weeks |
| **3** | Understand containers & Kubernetes from the inside | 4–6 weeks |
| **4** | Understand eBPF: the in-kernel VM, the verifier, maps | 6–10 weeks |
| **5** | Write eBPF in Rust with **aya** (our framework) | ongoing |
| **6** | The specialist domains, learned per-milestone | just-in-time |

Stages 0–5 overlap in practice — you'll be reading Rust while learning Linux. Don't serialize them
rigidly.

---

## Your dev box (Arch Linux)
You're in luck: **Arch is close to an ideal eBPF dev environment.** Its stock kernel is bleeding-edge
(far past the 5.8 ring-buffer floor), ships with **BTF on** (`CONFIG_DEBUG_INFO_BTF=y`, so
`/sys/kernel/btf/vmlinux` exists and CO-RE / `aya-tool` just work), defaults to the **unified cgroup
v2** hierarchy (exactly what M2 enrichment targets), and — unlike most hardened distros — already
lists **`bpf` in the default LSM stack** (`CONFIG_LSM="landlock,lockdown,yama,integrity,bpf"`), so
even the M6 BPF-LSM enforcement backend can attach **without** a boot-param change.

**Verify your box** — this is literally the agent's M0 boot preflight; run it now:
```bash
uname -r                       # ≫ 5.8 → fine
ls /sys/kernel/btf/vmlinux     # exists → CO-RE works
stat -fc %T /sys/fs/cgroup     # cgroup2fs → unified cgroup v2 (good)
cat /sys/kernel/security/lsm   # contains "bpf" → BPF-LSM enforcement available
```

**Toolchain (pacman + rustup + cargo):**
```bash
# eBPF + tracing tooling
sudo pacman -S --needed base-devel git llvm clang bpf bpftrace strace lsof qemu-base
#   bpf  → provides `bpftool` (aya-tool dumps BTF through it)
#   qemu → microVM verifier tests (lvh / vmtest)

# Rust: use rustup, NOT the `rust` package — the BPF target needs nightly + rust-src
sudo pacman -S --needed rustup
rustup default stable
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
cargo install bpf-linker       # the eBPF linker aya needs (links against system LLVM)

# Kubernetes (Stage 3, M2/M7): a runtime + kind + kubectl
sudo pacman -S --needed docker kubectl   # kind: `pacman -S kind` if present, else AUR `kind-bin`
```

**Two Arch gotchas:**
- **Develop on the stock `linux` kernel** (or `linux-lts` / `linux-zen`) — **not `linux-hardened`**,
  which locks BPF down (disables unprivileged BPF harder, restricts features) and will fight you.
- **Unprivileged BPF is off by default** (`kernel.unprivileged_bpf_disabled=2`), so run the
  `bpftrace` / program-load exercises with **`sudo`** (or grant `CAP_BPF`/`CAP_PERFMON`). Expected —
  the agent runs privileged in-cluster anyway.

> Because your kernel already satisfies the support matrix, the kernel-version and BTF caveats
> sprinkled through this doc are non-issues *locally for you* — but the agent must still handle nodes
> that lack them, which is exactly why M0 preflights and the support matrix exist.

---

## Stage 0 — Foundations a degree often skips
Before any of the hard stuff: you must be *fluent* in Linux and understand the boundary between a
program and the kernel (the **syscall**). Everything `agent` does lives at that boundary.

- **The Linux Command Line** (William Shotts) — free at
  [linuxcommand.org/tlcl.php](https://linuxcommand.org/tlcl.php). Become comfortable in a shell:
  files, processes, permissions, pipes. Non-negotiable baseline.
- **Linux Journey** — [linuxjourney.com](https://linuxjourney.com/). Friendly, hands-on tour of the
  system, the filesystem, processes, and `/proc` & `/sys` (where the kernel exposes itself).
- **Operating Systems: Three Easy Pieces (OSTEP)** — free at
  [ostep.org](https://pages.cs.wisc.edu/~remzi/OSTEP/). The best free OS book. Refresh
  processes, threads, virtual memory, scheduling, concurrency. This is the conceptual bedrock for
  *everything* below.
- **C, just enough to read it** — K&R's **The C Programming Language** (skim) + **Beej's Guide to C**
  ([beej.us/guide/bgc](https://beej.us/guide/bgc/)). You won't write much C, but the kernel and its
  docs *are* C — you must be able to read a `struct` and a function signature.
- **Syscalls — the program/kernel boundary** — **Beej's Guide to Network Programming**
  ([beej.us/guide/bgnet](https://beej.us/guide/bgnet/)) teaches sockets *and* the syscall mindset at
  once. When you call `open()` or `connect()`, you're asking the kernel to do something — that
  request is exactly what `agent` will observe.

> **Do:** open three terminals. Run `strace ls` and read which syscalls `ls` makes. Run `cat
> /proc/self/status`. Run `ps`, `top`, `lsof`. Get *curious* about what the kernel knows about your
> processes — that curiosity is the whole job.

## Stage 1 — Rust, from zero to systems-capable
Every crate in this repo is Rust. You need more than "it compiles" — you need ownership across the
FFI boundary, `unsafe`, `#[repr(C)]`, `no_std`, and real concurrency. Go in this order:

1. **The Rust Programming Language** ("the Book") — free at
   [doc.rust-lang.org/book](https://doc.rust-lang.org/book/). Read it cover to cover. Ownership,
   borrowing, lifetimes, traits, enums, error handling. This is the wall most people bounce off —
   push through it.
2. **Rustlings** — [github.com/rust-lang/rustlings](https://github.com/rust-lang/rustlings). Small
   exercises that make the Book stick. Do them alongside it.
3. **Programming Rust** (Blandy, Orendorff, Tindall — O'Reilly) — the deeper systems treatment; the
   best coverage of `unsafe`, memory layout, and FFI.
4. **Rust for Rustaceans** (Jon Gjengset) — intermediate→advanced: traits, generics, `unsafe`, FFI,
   API design. This is the level the shared `common` ABI and the loader actually demand.
5. **Rust Atomics and Locks** (Mara Bos) — free at [marabos.nl/atomics](https://marabos.nl/atomics/).
   Concurrency from first principles; you'll need it for the async event consumer and shared caches.
6. **Async & Tokio** — the [Tokio tutorial](https://tokio.rs/tokio/tutorial). The userspace agent is
   async; it polls the kernel's event buffer on Tokio.
7. **`no_std`, `#[repr(C)]`, FFI** — the [Rustonomicon](https://doc.rust-lang.org/nomicon/). The
   kernel-side crate is `no_std`; `common` is a C-layout type shared with the kernel side.

- **"Crust of Rust"** (Jon Gjengset, YouTube) — surgical deep-dives for when a concept won't click.

> **Do:** build two or three small CLIs in Rust (a log parser, a `/proc` scraper, a tiny TCP echo
> server). You should be able to read a file, spawn threads, and handle errors with `Result` without
> looking everything up.

## Stage 2 — Linux kernel & systems internals
Now go deeper into the system Rust will run on — the things eBPF hooks into: syscalls in detail,
processes, **cgroups** and **namespaces** (the building blocks of containers), and the kernel tracing
machinery (tracepoints, kprobes).

- **The Linux Programming Interface** (Michael Kerrisk) — the syscall/process/signals/sockets bible.
  You don't read it front-to-back; you live in it. The single most useful book here after the eBPF
  ones.
- **Linux Kernel Development** (Robert Love) — a readable tour of how the kernel itself is built
  (lighter than *Understanding the Linux Kernel* by Bovet & Cesati, which you can graduate to).
- Kernel docs, read as you need them: **tracepoints / kprobes / uprobes**
  ([docs.kernel.org/trace](https://docs.kernel.org/trace/)), **cgroup v2**
  ([docs.kernel.org/admin-guide/cgroup-v2](https://docs.kernel.org/admin-guide/cgroup-v2.html)),
  **namespaces** ([man7.org namespaces(7)](https://man7.org/linux/man-pages/man7/namespaces.7.html)).
- **LWN.net** — [lwn.net](https://lwn.net/). The best running commentary on kernel internals; read
  its feature articles when a topic (ring buffer, BPF-LSM) comes up.

> **Do:** create a cgroup by hand and watch a process's resource limits change. Use `unshare` to put
> a shell in a new namespace. When you realize *"a container is just namespaces + cgroups,"* Stage 3
> will feel obvious.

## Stage 3 — Containers & Kubernetes, from the inside
You're starting from zero on k8s, so don't start with `kubectl` memorization — start with *what a
container actually is* (you just learned: namespaces + cgroups), then how Kubernetes orchestrates
them, then the kernel-to-pod plumbing `agent` depends on.

- **Container Security** (Liz Rice) — builds containers up from namespaces/cgroups/capabilities/
  seccomp/LSM. The perfect bridge from Stage 2, and directly on-point for what `agent` secures.
  Pair it with her **"Containers from Scratch"** talk (search YouTube) — she builds one live.
- **Kubernetes Up & Running** (Burns, Beda, Hightower — O'Reilly) — the standard from-zero intro:
  pods, deployments, services, the object model. Plus the official
  [kubernetes.io tutorials](https://kubernetes.io/docs/tutorials/).
- **CRI + container runtimes** — containerd / CRI-O docs: how a container's **cgroup path encodes the
  pod/container id**. This *is* `agent`'s M2 enrichment join (`cgroup_id → containerID → pod`).
- **kube-rs** — [docs.rs/kube](https://docs.rs/kube) + [kube.rs](https://kube.rs/). The Rust k8s
  client; **reflectors/watchers** keep `agent`'s node-local pod cache fresh.
- **Programming Kubernetes** (Hausenblas & Schimanski) — informer/controller patterns (a reflector is
  one); read once you're comfortable with the basics.

> **Do:** install **`kind`** (Kubernetes in Docker) and run a cluster on your laptop. Deploy a pod,
> `kubectl exec` into it, then find that container's cgroup under `/sys/fs/cgroup`. You've just done
> the lookup `agent` automates.

## Stage 4 — eBPF foundations
The core skill. Learn what eBPF *is* — a safe VM inside the kernel, gated by a **verifier**, that
runs your code on events — before touching any framework.

- **Learning eBPF** (Liz Rice, O'Reilly 2023) —
  [O'Reilly](https://www.oreilly.com/library/view/learning-ebpf/9781098135119/) ·
  [examples repo](https://github.com/lizrice/learning-ebpf). **Start here.** Programs, maps, the
  verifier, the loader — all hands-on. The best on-ramp that exists.
- **ebpf.io** — [ebpf.io/what-is-ebpf](https://ebpf.io/what-is-ebpf/). The concept hub; read the
  "What is eBPF?" guide early.
- **BPF Performance Tools** (Brendan Gregg) — the reference. Tracing, probes, and *how to think about
  what's worth instrumenting*. The source of the kind of demos `agent` imitates.
- **bpftrace** — [github.com/bpftrace/bpftrace](https://github.com/bpftrace/bpftrace). A high-level
  tracing language: write a one-liner that fires on every `execve` *today*, before learning the hard
  way. The fastest path to intuition.
- **Cilium BPF & XDP Reference Guide** —
  [docs.cilium.io/.../bpf](https://docs.cilium.io/en/stable/reference-guides/bpf/) — and the **kernel
  BPF docs** ([docs.kernel.org/bpf](https://docs.kernel.org/bpf/)): the deep references for program
  types, map types, helpers, and the verifier. Use them as dictionaries, not cover-to-cover.
- **CO-RE & BTF — "compile once, run everywhere"** (why one build runs across kernel versions):
  Andrii Nakryiko's **"BPF Portability and CO-RE"**
  ([nakryiko.com](https://nakryiko.com/posts/bpf-portability-and-co-re/)) and the
  **"CO-RE reference guide"** ([nakryiko.com](https://nakryiko.com/posts/bpf-core-reference-guide/)),
  plus Brendan Gregg's [BTF/CO-RE overview](https://www.brendangregg.com/blog/2020-11-04/bpf-co-re-btf-libbpf.html).
- **"The BSD Packet Filter"** (McCanne & Jacobson, 1993) — the origin paper; read it once to
  understand *why* the VM and verifier exist.

> **Do:** write `bpftrace` one-liners that trace `execve`, `tcp_connect`, and `openat`. When you can
> see processes, connections, and file opens scroll by from a single line of code, eBPF has clicked.

## Stage 5 — aya: eBPF in Rust (our framework)
`agent` is built on **aya** — pure-Rust eBPF, *both* the kernel programs and the userspace loader.
No C, no libbpf. This is where the previous stages converge.

- **The Aya Book** — [aya-rs.dev/book](https://aya-rs.dev/book/). **Primary.** Build/load/attach,
  maps, the two-crate layout (`ebpf` is `no_std` + a userspace loader), and `aya-tool` for generating
  kernel-struct bindings. Work every example.
- **docs.rs** — [`aya`](https://docs.rs/aya) (userspace) and [`aya-ebpf`](https://docs.rs/aya-ebpf)
  (kernel side). The API you'll live in.
- **aya-rs/aya** source ([github.com/aya-rs/aya](https://github.com/aya-rs/aya)) + **awesome-aya**
  ([github.com/aya-rs/awesome-aya](https://github.com/aya-rs/awesome-aya)) — real programs to imitate.
- Dave Tucker, **"Improving the eBPF Developer Experience with Rust"** (Linux Plumbers talk) — why
  aya exists and why it avoids libbpf/BCC.

> **Do:** complete the Aya Book's tracepoint example end-to-end on your own machine. That *is* a
> miniature of ROADMAP **M1**. From here, you start building `agent` for real.

## Stage 6 — The specialist domains (learn per-milestone)
Don't front-load these — pull each in when its milestone arrives.

- **LSM-BPF & enforcement** (M6 — how to *deny*, not just observe): **KRSI** ("Kernel Runtime Security
  Instrumentation") — the [LWN article](https://lwn.net/Articles/798157/) + the BPF-LSM kernel docs;
  the `bpf_send_signal` (kill) and `cgroup/connect4|6` (egress deny) helpers.
- **Networking** (M3 — the `connect` probe): one of **TCP/IP Illustrated Vol. 1** (Stevens/Fall) or
  **Computer Networking: A Top-Down Approach** (Kurose/Ross) for the protocol model; you'll read
  `struct sock` fields from the kernel via CO-RE.
- **GPU / CUDA / AI-inference internals** (M4 — *the wedge*; generic tools are blind here):
  - **NVML** + **DCGM** (NVIDIA docs) and the **`nvml-wrapper`** crate
    ([docs.rs/nvml-wrapper](https://docs.rs/nvml-wrapper)) — per-process GPU util/memory; study
    **dcgm-exporter** ([github.com/NVIDIA/dcgm-exporter](https://github.com/NVIDIA/dcgm-exporter)).
  - **Programming Massively Parallel Processors** (Kirk & Hwu) — *just enough* CUDA model to know what
    `cudaLaunchKernel` / `cudaMalloc` mean (the calls M4's uprobes count).
  - The eBPF-on-GPU frontier (2025, the novel angle): "Snooping on your GPU / **GPUprobe**"
    ([dev.to](https://dev.to/ethgraham/snooping-on-your-gpu-using-ebpf-to-build-zero-instrumentation-cuda-monitoring-2hh1)),
    eunomia's **"The GPU Observability Gap"**
    ([eunomia.dev](https://eunomia.dev/blog/2025/10/14/the-gpu-observability-gap-why-we-need-ebpf-on-gpu-devices/)),
    the **eGPU** paper ([ACM HCDS '25](https://dl.acm.org/doi/10.1145/3723851.3726984)), and
    **"GPU Observability with eBPF"** ([annanay.dev](https://annanay.dev/gpu-observability-using-ebpf/)).
  - **vLLM docs** — its Prometheus metrics (tokens/sec, queue depth, KV-cache); the inference KPIs M4
    ties to pods.
- **gRPC / protobuf** (M7 — the contract `agent-cloud` consumes): [protobuf.dev](https://protobuf.dev/)
  + **tonic** ([docs.rs/tonic](https://docs.rs/tonic)) + **prost** ([docs.rs/prost](https://docs.rs/prost)).

## Stage 7 — Study the giants (start as soon as you can read eBPF)
Reading real systems teaches architecture faster than any book. Don't wait until the end.

- **bpfman** — [github.com/bpfman/bpfman](https://github.com/bpfman/bpfman). CNCF, Rust + aya + k8s:
  our **closest match — study it first.**
- **cilium/tetragon** — eBPF security + k8s enrichment; the **best architectural model** for what we
  build (Go, but the architecture is the lesson).
- **falcosecurity/falco** — the detection/rules engine model (M5).
- **kubearmor/KubeArmor** (eBPF + LSM enforcement, M6), **aquasecurity/tracee**, **pixie-io/pixie**,
  **parca-dev/parca-agent** — adjacent designs, a read each.

---

## Just-in-time: what to learn before each milestone
This is the real plan. Learn the *slice*, build the milestone, loop back. Numbers/stars match
[`ROADMAP.md`](./ROADMAP.md). Stages 0–5 are prerequisites for *starting at all* — finish enough of
them to be dangerous before M0.

| Milestone | Learn first (minimum viable) |
|-----------|------------------------------|
| **M0 Scaffold & CI** | Stage 1 (the Rust Book) · Stage 5 (Aya Book ch. 1–3 + toolchain: `bpf-linker`, the nightly BPF target) · Stage 4 (the CO-RE/BTF idea, ebpf.io) |
| **M1 First probe (exec)** ⭐ | Stage 4 (program types, maps, the **ring buffer**) · Stage 2 (**tracepoints**) · Stage 1 (`#[repr(C)]` / `no_std`) · Stage 5 (the loader: `Ebpf::load` + attach) |
| **M2 k8s enrichment** ⭐ | Stage 2 (**cgroup v2**) · Stage 3 (**CRI / cgroup→pod**, **kube-rs reflectors**, *Container Security*) · Stage 1 (shared-state concurrency, Atomics) |
| **M3 Network + file probes** | Stage 2 (**kprobes/fentry**) · Stage 6 networking (sockets, `struct sock`) · Stage 4 (in-kernel filtering with maps) |
| **M4 GPU/AI telemetry** ⭐ | Stage 6 GPU (**NVML/DCGM** + `nvml-wrapper`, CUDA model, **uprobes**, the eBPF-GPU reading, vLLM metrics) |
| **M5 Detection engine** | Stage 7 (**Falco's** rules model) · Stage 1 (enum/trait design for the rule types) |
| **M6 Enforcement (optional)** | Stage 6 (**BPF-LSM / KRSI**, `bpf_send_signal`, `cgroup/connect`) · Stage 7 (KubeArmor) |
| **M7 Exporter + packaging** | Stage 6 (**tonic/prost/protobuf**) · Stage 3 (RBAC, securityContext/capabilities — the **CKS** material) |

---

## Build the muscle (do, don't just read)
Each is also listed under its stage; collected here as the practical spine:

1. **`strace` a few programs** and read `/proc` — internalize the syscall boundary (Stage 0).
2. **Build a container by hand** with `unshare` + cgroups — namespaces + cgroups *are* containers
   (Stages 2–3).
3. **`bpftrace` one-liners** on `execve` / `tcp_connect` / `openat` — eBPF intuition before aya
   (Stage 4).
4. **Run the Aya Book's tracepoint example** end-to-end — a tiny M1 (Stage 5).
5. **Reproduce the signature demo** — catch a shell spawned in a pod and name it. That's **M1 + M2**.
6. **Kernel testing in a microVM** — `lvh` (little-vm-helper) or `vmtest` load programs against a
   known kernel; it's the M0 CI gate, so learn it early.
7. **A `kind` cluster** for the enrichment and RBAC work (M2, M7); a **spot GPU node** when you reach
   M4 (mock the GPU collector until then).

---

## Gotchas — the production dragons
Traps that don't show up in tutorials but *will* bite during implementation. Each is already handled
in [`ROADMAP.md`](./ROADMAP.md) / [`.rules`](./.rules) — internalize the *symptom* now so you
recognize it when it appears, instead of losing a day to it.

1. **The verifier rejects uninitialized padding.** Build an event on the BPF stack and submit it to
   the ring buffer and the verifier throws *"invalid indirect read from stack"* — because C alignment
   left an uninitialized padding byte, and the verifier tracks every byte's provenance to stop kernel
   memory leaking to userspace. *Lesson:* zero the reserved ring-buffer slot before writing, keep
   `#[repr(C)]` types padding-free (u64s first), `const`-assert the layout. (M1, `crates/common`.)
2. **Short-lived pods ghost.** A 45 ms `cronjob` execs `ls` and exits; by the time userspace resolves
   its `cgroup_id` → pod, the cgroup directory is already unlinked and resolves to nothing. *Lesson:*
   capture a second, slower-recycling identity key in-kernel — the mount-namespace inode
   (`mnt_ns_inum`) — and reconcile on the composite key. (M2.)
3. **`libcudart` uprobes silently no-op.** Hooking `cudaLaunchKernel` works on laptop PyTorch and
   fails on most production serving images, because vLLM/TGI/TensorRT-LLM static-link CUDA into a fat
   `.so` — there's no `libcudart.so` to attach to. *Lesson:* hook the NVIDIA driver **ioctl boundary**
   (`/dev/nvidia*`) for attribution (immune to static linking), keep DCGM/NVML for the values. (M4.)
4. **BPF-LSM won't attach if it isn't in the LSM list.** On hardened distros (Flatcar, Ubuntu Pro,
   RHEL) with AppArmor/SELinux primary, `BPF_PROG_TYPE_LSM` programs fail to *attach* (`EINVAL`)
   unless `bpf` is in the boot `lsm=` list — a hard precondition, not a silent runtime skip. *Lesson:*
   preflight `/sys/kernel/security/lsm` at boot; if `bpf` is absent, WARN and degrade to
   `bpf_send_signal` + cgroup enforcement. (M6.)
5. **Cold start: events arrive before the cache exists.** Your DaemonSet lands on a node with 110
   pods already running and passing traffic before the kube-rs reflector's first `List` returns.
   *Lesson:* never gate capture on enrichment — attach probes first, seed the baseline from `/proc`,
   node-scope the List, park cache-misses in a bounded resync queue that emits `Unknown` rather than
   dropping, and flag every event `synced: cold_start | steady`. (The cold-start contract in `.rules`.)
6. **Two silent foot-guns.** A ring-buffer size that isn't a power-of-two **and** page-multiple →
   `bpf_map_create` returns `-EINVAL` with no explanation. A `statx` crawl over every cgroup leaf file
   (`cpu.pressure`, `memory.stat`) on a dense node → a CPU spike *in your own agent*. *Lesson:*
   validate the size in the loader; scan directories only, under `kubepods.slice`, on a low-priority
   task. (M0/M2.)

---

## Certs & where to start tomorrow

**Certs that align** (and give your learning external structure): the **Linux Foundation's**
intro courses, then **CKA** (Kubernetes administration) and eventually **CKS** (Kubernetes security —
which M2/M5/M6 directly exercise). These aren't required to build `agent`, but for someone starting
from zero they're a well-paced syllabus for Stages 0 and 3, and they're résumé-legible.

**Your first month, concretely:**
1. **The Linux Command Line** + `strace ls` until the syscall boundary feels real — Stage 0.
2. **The Rust Programming Language** + **Rustlings**, in parallel — Stage 1.
3. **Install `bpftrace`** and trace `execve` in one line — a taste of Stage 4 to stay motivated.
4. Skim **Learning eBPF** ch. 1–2 and **ebpf.io** to see where it's all heading.

Then keep going down the stages, and **start M0 the moment you can complete the Aya Book's
tracepoint example.** Don't wait to feel "ready" — you learn the rest by building.
