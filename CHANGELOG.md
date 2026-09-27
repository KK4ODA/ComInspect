# Changelog

All notable changes to ComInspect are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The release workflow publishes the
section matching the tag as the release notes shown in the app.

## [0.3.0] - 2026-09-27

See how VSPE connects your ports.

### Added

- **VSPE splitters as a tree.** ComInspect reads the configuration of Eterlogic's Virtual Serial
  Ports Emulator (VSPE) and shows each splitter's virtual ports under the port they share, in the
  port list and in the details panel, together with the program on each of them. VSPE pairs,
  connectors, redirectors and network devices are explained in the details panel too. ComInspect
  reads VSPE's startup configuration automatically, or a `.vspe` file you choose, and never changes
  VSPE.
- `cominspect-cli vspe [FILE]` lists what a VSPE configuration connects.

### Changed

- Ports from known virtual-port software (VSPE, com0com, Eltima and others) are recognized as
  virtual whatever bus Windows lists them on.

## [0.2.0] - 2026-09-27

See which program has a port open, and get told when it lets go.

### Added

- **Which program is using each port.** The port list shows an "in use" badge, and the details
  panel has a "Programs using this port" section with the program's name (for example VARA FM),
  since when it has had the port, and whether it is shutting down. ComInspect reads this from the
  operating system without opening the port, so it never keys a radio.
- **Notify me when it's free.** A desktop notification the moment a port is released, and
  optionally a program to start then, for example VarAC once VARA FM has let go of the port. The
  program is remembered for each device. ComInspect asks before closing while it is waiting.
- **Command-line tool downloads** for Windows, macOS and Linux, with two new commands:
  `cominspect-cli who` lists the programs using each port, and
  `cominspect-cli wait-free COM4 --then PROGRAM` waits until a port is free, for scripts and
  batch files.

### Changed

- The open test, CAT query and PTT test name the program using a port, and leave the port alone,
  on Windows and macOS too (before, only on Linux).
- The update dialog's "What's new" text now comes from this changelog.

## [0.1.0] - 2026-09-27

First preview release.

- Serial-port inventory for Windows, Linux and macOS with persistent device
  identities (USB serial number, Bluetooth address, OS instance, USB socket)
  that survive reconnection and COM-number changes.
- Windows: hidden (previously connected) ports, reserved COM numbers, stale
  reservations, duplicate assignments and driver problems, with explanations.
- Nicknames, equipment, purpose (CAT, PTT, KISS, GPS …), CAT verification,
  categories and notes for every port.
- Live updates when devices are plugged in or removed.
- Hints for common USB-UART chips, including the Enhanced/Standard ports of
  dual-port bridges and radios that identify themselves by USB serial string.
- Safe, explicit diagnostics: open test, read-only CAT query with automatic
  baud detection, confirmed PTT test.
- Export/import of port mappings between computers.
- Automatic, signed updates with stable and beta channels.
