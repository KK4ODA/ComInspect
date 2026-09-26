# Changelog

All notable changes to ComInspect are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). The release workflow publishes the
section matching the tag as the release notes shown in the app.

## [0.1.0] - Unreleased

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
