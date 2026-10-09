# Argus — getting started



Rust stable and Cargo are required. `rust-toolchain.toml` selects stable; the workspace declares Rust 1.78 as its minimum, but the published run uses Rust 1.95.0 (the declared minimum was not validated).

```bash
git clone https://github.com/yMScorpion/Argus.git
cd Argus
cargo test --workspace --locked
cargo build --workspace --locked
# Optional benchmark run; do not compare numbers across machines without context:
cargo bench --workspace
```

The test suite uses fixtures and temporary local files. No exchange credentials are needed. See [Getting started](GETTING_STARTED.md) for useful focused commands.


## Reproduction record

Use [VERIFICATION.md](VERIFICATION.md) to compare the code revision, toolchain and command. Never store real credentials in tracked files.
