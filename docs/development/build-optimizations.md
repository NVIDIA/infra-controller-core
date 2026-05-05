# Build Optimizations and Trade-offs

The Docker release image build (`Dockerfile.release-container-sa-x86_64`) includes several
non-obvious optimizations. This page documents the intent and trade-offs so future maintainers
understand why the build is structured the way it is.

## `debug = "line-tables-only"` in the release profile

**What it does:** The release profile uses `debug = "line-tables-only"` instead of the Rust default
(`debug = 0`) or full debug info (`debug = true`). This embeds line-number tables in binaries but
omits DWARF variable info (local variable names, types, values).

**Why:** With `debug = true`, the `carbide-api` binary alone was ~1.46 GB, producing a 5.4 GB
release image. `"line-tables-only"` reduced the binary to ~544 MB and the image to ~2.5 GB —
a 58% reduction — while keeping stack traces useful (line numbers are preserved).

**Trade-off:** Debuggers (gdb/lldb) and core dump analysis will show the call stack with line
numbers but will not be able to inspect local variable values. For production debugging this is
usually acceptable because we rely on structured logging and tracing rather than debugger sessions.
If you need full variable inspection (e.g., for post-mortem core analysis of a reproduction), build
locally with `debug = true` in a `[profile.dev]` override or a local `Cargo.toml` override.

Release container builds override the default back to full debug info via the
`CARGO_PROFILE_RELEASE_DEBUG=true` environment variable in the Dockerfiles.

## `--no-workspace` on `clippy-release` and `build-release`

**What it does:** Both tasks are invoked with `cargo make --no-workspace` in the Dockerfile.
Without this flag, cargo-make iterates all workspace members (64 crates) and calls `cargo build`
or `cargo clippy` once per crate. With `--no-workspace`, cargo-make runs the task once at the
workspace root, which is equivalent to running `cargo build --workspace` — a single invocation
that builds everything once.

**Why:** The per-member iteration caused shared dependencies (`tonic`, `sqlx`, `carbide-rpc`, etc.)
to be recompiled repeatedly across members. Switching to `--no-workspace` reduced the build from
~98 minutes to ~21 minutes on a 72-core server.

**Trade-off:** Per-crate feature isolation is lost. Cargo unifies features across all workspace
members at once rather than resolving each crate independently. For this project all crates ship
together as a single release image, so cross-crate feature conflicts are not a concern. If you
ever extract a crate for standalone deployment, validate its feature set independently with
`cargo build -p <crate>`.

## `clippy-release` shares artifacts with `build-release`

**What it does:** The `clippy-release` Makefile task runs clippy with `--release`, and
`build-release` also compiles in release mode. Because they share the same compilation flags and
profile, the compiled `.rlib` and `.rmeta` artifacts from the clippy step are reused by the build
step — no second compile of all 64 crates.

**Trade-off:** `clippy-release` passes `--all-targets`, which includes test and benchmark targets
that `build-release` does not compile. Clippy therefore lints slightly more code than is shipped
in the final binary. In practice this is a net benefit (broader coverage), but if a test-only
dependency activates features that interact unexpectedly with production code, the lint results
may differ from a targeted per-crate clippy run.
