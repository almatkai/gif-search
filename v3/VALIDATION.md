# Validation and remaining limits

Recorded on 2026-10-25 using macOS 26.3 ARM64, rustc 1.98.0,
`aarch64-apple-darwin`. Local evidence does not establish Windows or macos-15
runner behavior. The native workflow is authored; the parent must run and review
both native CI targets before platform release.

SDK: immutable git revision `9f91afcc071c3f301414dc62b33cfeac7d89a0cf`.
Package schema 3, JSONL protocol 1. No app/Tauri dependency or runtime toolchain.
Source `artifacts` stays empty until reviewed artifacts are actually distributed.
ZIP and executable hashes live in adjacent `.artifact.json`; the full ZIP file
inventory is in `.inventory.json` and runtime members in `extension.json.files`.
These are local artifacts, not proof of publication, signatures or host installation.

## Local checks

The focused review passed 25 Rust tests (19 unit + 6 Rust pipe tests),
with zero failures/ignored tests, including all 14 original adapter regressions.
Locked production release, strict Clippy and formatting passed. Deterministic ZIP
comparison and isolated extracted executable smoke are required packaging checks.
To reproduce from this directory:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -j1 -- -D warnings
cargo test --locked --all-features -j1 -- --test-threads=1
cargo build --locked --release -j1
```

Use the package and smoke commands in README on the matching native platform.
All fixture stores and socket listeners are temporary; no live user credentials,
provider writes, installed extension data or user listener enumeration are tested.

## Coverage and unverified integration

The review adds rejection of relative/directory/symlink/special migration files.
Fixtures cover providers, pagination, collections/migration/corruption, GIF frames,
delays/repeat, cached media, download naming and typed reverse RPC cancellation.

Live Raycast GIPHY/Klipy proxy availability/permission outside Raycast, FinerGifs
availability, real desktop file clipboard/paste, and clean-profile Tezbar install
remain unverified. No provider credentials are bundled or invented.
`host.clipboard.writeFiles {paths:[absolutePath]}` must copy the GIF/MP4 file
reference with animation intact and propagate OS failure. Rasterizing to an image
is insufficient. Temporary clipboard file lifetime must outlast host ownership;
real paste/recipient behavior still needs host QA. Codec work is synchronous with
bounded dimensions/frame/pixel totals and cancellation checks between frames;
a single codec step or atomic file transaction can occupy a runtime worker.
The host must enforce hard deadlines and process-tree teardown. Migration grants
are exact absolute regular non-symlink files; host also fences parent directories.
