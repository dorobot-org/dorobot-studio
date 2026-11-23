# dorobot-nav-bridge

Dora node that republishes DORA_NAV state to Dorobot Studio over Zenoh using the
`dviz-protocol` wire format (topics `dviz/<robot_id>/<entity>`).

This directory is its own Cargo workspace (own `Cargo.lock`, Dora 1.1 / Zenoh 1.10.2). It is
not part of the Studio root workspace and is independent of `runtime/` (Dora 1.0.1).

Status: the pure logic is done and tested (`config`, `formats`, `geom`, `map`, `trail`,
`rate`, `status`, `encode`). The Zenoh publisher and the Dora event loop are added in later
tasks; `main` only prints usage for now.

```bash
export TMPDIR=$HOME/nvme/dora-lab/tmp
cargo test --locked
cargo clippy --locked --all-targets
```

`config/tracer.json` holds the Tracer Mini defaults (listen address, map files, footprint,
input name to format table, rate caps, colours).
