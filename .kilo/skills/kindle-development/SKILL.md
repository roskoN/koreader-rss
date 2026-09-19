---
name: kindle-development
description: Build, deploy, or debug Ornith on its ARMv7 jailbroken Kindle target. Use for cross compilation, QEMU checks, SSH deployment, and device-only failures.
---

# Kindle target workflow

Treat the Kindle as an ARMv7, resource-constrained target. First identify
whether the failure is host-only, cross-build, emulation, or device-only.

## Bounded loop

1. Build the smallest affected Rust artifact with `cross` (or the repository's
   configured cross toolchain).
2. Use `qemu-arm`/`qemu-arm-static` for a quick executable or ABI check when it
   can reproduce the problem.
3. Deploy only the required artifact through the established `ssh kindle` and
   `scp`/`rsync` path; do not overwrite unrelated device state.
4. Capture a short, relevant device log and compare it with the host result.

Keep binaries, memory use, CPU work, and image dimensions conservative. Validate
paths, executable permissions, and linked-library assumptions on device. Never
turn a device deployment into a broad cleanup or configuration rewrite.
