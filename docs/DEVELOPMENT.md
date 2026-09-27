# Developing ComInspect

This page covers building from source, the project layout, tests and common changes. For how the
application works internally, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Prerequisites

- **Rust** through [rustup](https://rustup.rs). The Rust version is pinned in `rust-toolchain.toml`,
  and rustup installs it automatically the first time you run `cargo` in the repository.
- **Node.js 22** or newer, with npm.
- **Platform tools:**

  | System | Install |
  |---|---|
  | Windows | Visual Studio Build Tools with the *Desktop development with C++* workload. WebView2 is already present on Windows 11 and up-to-date Windows 10. |
  | macOS | Xcode command line tools (`xcode-select --install`). |
  | Debian/Ubuntu | `sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev patchelf` |
  | Fedora | `sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel libappindicator-gtk3-devel patchelf` |

  The libraries and the command-line tool need only Rust. The WebKit/GTK packages are needed only
  for the desktop app.

## Getting started

```sh
git clone https://github.com/KK4ODA/ComInspect.git
cd ComInspect
npm ci

npm run dev              # the UI in a browser, with built-in demo data: http://localhost:5173
npm run tauri dev        # the real desktop app, with hot reload of the UI
cargo run -p cominspect-cli -- list   # scan from the command line
```

- **The demo backend.** In a plain browser, the UI detects that it is not inside the Tauri app and
  uses a built-in demo backend (`ui/src/lib/mock.ts`). The demo shows a typical station and needs
  no serial hardware. It is handy for UI work, and the documentation screenshots come from it.
- **Installers.** To build installers for your own platform, run `npm run tauri build`. The files
  appear in `target/release/bundle/`.
  - Local builds don't create update files. Release builds turn them on with
    `--config src-tauri/tauri.release.conf.json`, which needs the signing key.

## Project layout

```
crates/
  cominspect-core/       platform-independent model, identity engine, device hints,
                         scan analysis (warnings), export format
    data/known-devices.json   the built-in device hint database
  cominspect-store/      SQLite database: schema and migrations, backups,
                         reconciliation of scans with known devices, import/export
  cominspect-platform/   serial port discovery and change monitoring per OS
                         (windows/, linux/, macos/), which programs have a
                         port open (usage/), starting a program (launch.rs),
                         plus explicit diagnostics (open test, CAT queries,
                         PTT test)
  cominspect-cli/        the `cominspect-cli` command-line tool
src-tauri/               the desktop app: Tauri commands, app state, updater,
                         port usage watcher, launch bookkeeping; tauri.conf.json
ui/                      Svelte 5 + TypeScript front end
  src/lib/               state (app.svelte.ts), backends (Tauri and demo),
                         labels, filters, formatting, types
  src/components/        table, details panel, dialogs, status bar
scripts/                 version bump and updater manifest check
docs/                    documentation
```

Nothing in `crates/` depends on Tauri. The desktop app is a thin layer over the libraries, and the
command-line tool uses the same libraries.

## Checks and tests

Run these before pushing. CI runs the same checks on Windows, macOS and Linux.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --locked                  # libraries and CLI
npm run build && cargo test -p cominspect --locked   # desktop app
npm run check                        # Svelte and TypeScript type check
npm test                             # front-end unit tests
```

### What the tests cover

- **`cominspect-core`:**
  - identity keys and matching, including serial-number hygiene, conflict vetoes and duplicates
  - device hints
  - scan analysis
  - the export format
  - VSPE configuration files, including one saved by VSPE 1.5 (`tests/data/splitters.vspe`)
- **`cominspect-store`:**
  - the device lifecycle: new, disconnected, reconnected on another port, merged, forgotten
  - migrations and newer-schema handling
  - backups and restore
  - import and export round trips
- **`cominspect-platform`:**
  - Linux discovery against sysfs fixtures, including USB, platform UARTs, Bluetooth RFCOMM and
    virtual ports
  - interpretation of Windows device properties and macOS IORegistry data from recorded values
  - CAT reply parsing
  - which process holds a port: a pseudo-terminal held by a child process (Linux, macOS), and on
    Windows a real serial port (below)
- **The desktop app:** launch and recovery bookkeeping, and update channels.

The Windows and macOS system calls themselves run in CI on real Windows and macOS machines, which
execute `cominspect-cli list --json` as a smoke test. They also hold a port from another process
and check that `cominspect-cli who` names it and that `wait-free` returns once it is released.

The Windows tests that need a real serial port read its name from `COMINSPECT_TEST_PORT`; CI sets
it to the runner's `COM2`, and without it they are skipped. To run them on your own computer, name
a port that nothing else is using:

```powershell
$env:COMINSPECT_TEST_PORT = "COM3"; cargo test -p cominspect-platform usage
```

### Checking Windows and macOS code from Linux

The platform code can be type-checked and linted for other operating systems without their SDKs:

```sh
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin
cargo clippy -p cominspect-platform -p cominspect-core --all-targets \
  --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy -p cominspect-platform -p cominspect-core --all-targets \
  --target aarch64-apple-darwin -- -D warnings
```

The store crate can't be cross-checked this way, because its bundled SQLite is C code that needs
the target's C compiler.

### Testing diagnostics without a radio

On Linux and macOS, a pseudo-terminal can stand in for a radio. For example, create a pair with
`socat -d -d pty,raw,echo=0 pty,raw,echo=0`. Then answer `ID;` on one end with a short script, and
point `cominspect-cli cat <other end> --protocol kenwood-id` at the other.

## Logging

- **Where logs go.** The desktop app writes `cominspect.log` to the log folder (**Menu → Open log
  folder**) and to standard output.
- **Log level.** The level is `info` in release builds and `debug` in development builds. Set
  `COMINSPECT_LOG=debug` (or `trace`, `warn`) to override it.

## Common changes

### Adding a device hint

Hints live in `crates/cominspect-core/data/known-devices.json` and are compiled into the app. Each
entry has a `match` block and the text to show:

```json
{
  "id": "vendor-product",
  "match": { "vid": "10C4", "pid": "EA60", "serialPrefix": "IC-" },
  "chip": "Silicon Labs CP210x",
  "title": "Icom {serialModel}",
  "detail": "What this port is, stated only as far as the IDs establish it.",
  "shortLabel": "Icom {serialModel}",
  "equipment": "Icom {serialModel}",
  "purpose": "cat",
  "category": "radio_cat",
  "caution": false
}
```

**Matching and ordering**

- The `match` fields are: `vid`, `pid`, `interface`, `serialPrefix`, `productContains`,
  `descriptionContains`, `nameContains`, `transport`, `driver`, `platform` and `virtualProvider`.
  All given fields must match.
- The most specific matching entry is shown first.

**Text**

- `{serialModel}` and `{product}` in the text are replaced with values from the device.
- `shortLabel` is the grey tag in the port table.
- `caution: true` shows the hint as a warning, for example for chips that are often counterfeit.

**Suggestions**

- `purpose`, `category` and `equipment` are offered as suggestions and never applied
  automatically.
- Only suggest a purpose that the USB IDs reliably establish. When in doubt, explain in `detail`
  instead.

Run `cargo test -p cominspect-core` after editing. A test loads the bundled file, and it fails on
unknown fields, invalid hex IDs or anything else that doesn't parse.

### Changing the database schema

Migrations live in `crates/cominspect-store/src/schema.rs`.

1. Append a `Migration` to `MIGRATIONS` with the next `version`, and raise `SCHEMA_VERSION` to
   match.
2. Set `compat` to decide whether older versions of ComInspect may keep using the database:
   - **Additive change** (a new table, or a new nullable column that older builds can ignore): keep
     `compat` at the previous value. Older builds can then keep writing to the database, for
     example after a user reinstalls the previous version.
   - **Breaking change** (renamed or retyped columns, or changed meaning): set `compat` to the new
     version. Older builds then refuse to write to the database, run on a temporary database
     instead, and show a banner.
3. Never edit a migration that has shipped. Fix mistakes with a new migration.
4. Add a test in `crates/cominspect-store/src/tests.rs` that opens a database at the previous
   schema and checks that the data survives.

Before migrating, the app backs up the database automatically to
`backups/inventory-schema<N>-<time>.db`.

### Updating the Rust version

1. Change `channel` in `rust-toolchain.toml`.
2. Run the clippy and test commands above, and fix any new lints in the same commit.

CI and release builds read the version from that file.

### Changing the version number

Run `npm run version:set -- X.Y.Z` (see [RELEASING.md](RELEASING.md)). The version appears in
`Cargo.toml`, `package.json` and both lockfiles, and the release workflow checks that they match
the tag.
