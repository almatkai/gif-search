# GIF Search native process

Standalone Rust implementation of Search, Favorites and Recents. The original
TypeScript package remains available in the repository root. This directory
builds an independent executable with the frozen Tezbar SDK; no app, Tauri, Bun
or JavaScript runtime is required by the installed package.

Run `cargo test --locked --all-features -j1`,
`cargo clippy --locked --all-targets --all-features -j1 -- -D warnings`, and
`cargo build --locked --release -j1` from this directory. Standard I/O is the
SDK protocol 1 JSONL transport, schema 3. `--protocol-info` reports identity and
commands. The host starts the process lazily and supplies private package,
data and temporary directories. Catalog artifacts remain empty until the
parent approves and publishes actual archives.

All original command preferences and native domain behavior are preserved:
GIPHY GIFs/Clips, Klipy cursor paging, Finer search (no trending endpoint),
provider ID lookup, favorites, recents, usage ordering, cached favorites,
media copy/paste, link/Markdown copy/paste, details, square animation conversion,
non-overwriting downloads and open/reveal. JSON responses are capped at 8 MiB;
media at 100 MiB; search at 4096 bytes; result limits at 1–50. GIF conversion
preserves frame delays and repeat and limits dimensions to 4096, 2000 frames
and 128 million decoded pixels. Failed providers display explicit errors and
Retry; no synthetic results are returned.

The production provider URLs preserve the source contract. Raycast's proxy
availability/permission outside Raycast is not established by source inspection
or fixture tests. Direct GIPHY/Klipy APIs use separate authentication/contracts;
this extraction does not substitute them or embed keys. Finer's contract is
preserved from the original TypeScript/native source. No live provider requests
or user account actions are required by tests.

Storage is SDK atomic JSON at `<dataDir>/gif-search/storage.json`; malformed
JSON, collection types, provider selections, migration markers, snapshots and usage
are rejected without overwriting.
Only explicit migrationPaths keys are read:

- `gif-search.legacy-storage`: old extension `.tezbar-local-storage.json`.
- `gif-search.legacy-package-storage`: old raycast package local storage file.
- `gif-search.native-v2-store`: old `native-extensions-v2/gif-search/storage.json`.

The first two import only the eight provider favs/recent lists and selected
service. The native store additionally imports validated `gifSearchItems` and
`gifSearchUsage`. Unknown source keys, preferences, API keys and app config are
not imported. Existing destination keys win and unknown destination fields
survive. `_gifSearchV2Imported` fences imports. Grant paths before first session;
no implicit filesystem probing occurs. Cache files are recreated on demand.

Media clipboard uses `host.clipboard.writeFiles {paths:[absolutePath]}` and awaits
an error-free acknowledgement before recording success. Animation is not
rasterized through writeImage. Clipboard reads/writeImage are not used by GIF
Search. Text/paste/open/reveal/window operations use frozen SDK typed RPCs.
The host must implement writeFiles on each OS and enforce grants. Actual desktop
clipboard/paste was not exercised; pipe tests acknowledge requests in a fixture.

Downloads preserve the absolute `downloadPath` preference, `~/` expansion and
HOME/USERPROFILE Downloads default. Tests override the home and download paths
with temporary directories. The host must preserve the user's actual home
environment or require an explicit absolute download preference. Downloads do
not overwrite existing files. No downloads happen until the user invokes them.

HTTP jobs are bounded, registered through `ctx.spawn`, canceled on replacement,
dispose, EOF and shutdown, and invalidate their owning session. Session state and
preferences use SDK snapshots; refresh remains a 500ms fallback. The bounded
GIF encoder checks a cancellation flag between frames, driven by a scoped
SDK-owned watcher that marks the flag on drop. Atomic file transactions run in the handler with no detached
worker. A single bounded frame/file transaction can occupy a Tokio worker; host
deadlines and process-tree teardown remain authoritative for hard cancellation.

`fixture-http` is test-only: it permits a 127.0.0.1 HTTP origin supplied by
TEZBAR_GIF_FIXTURE_ORIGIN, never a preference. Actual executable tests exercise
HTTP and JSONL using this build. Release builds omit it, and the packager rejects
fixture-enabled binaries using `--build-info`.

On a native build host, package with
`python3 scripts/package.py --target aarch64-apple-darwin --binary target/release/tezbar-gif-search`.
Use the Windows target/executable on Windows. ZIPs contain root extension.json,
bin, resources, licenses, provenance, LICENSE/NOTICE and this README. The script
writes deterministic ZIPs, SHA-256 inventory sidecars and external catalog
fragments. `scripts/smoke-package.py` validates/extracts the ZIP, launches it with
no developer/toolchain PATH and runs a two-way fixture host. It never accesses
the desktop clipboard, browser or user directories. CI builds macOS ARM and
Windows x64 natively and uploads review artifacts; it does not publish releases.
Windows execution is unverified until that job actually runs.
