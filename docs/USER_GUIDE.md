# ComInspect user guide

ComInspect shows every serial port on your computer and the device behind each one. It remembers
the names and notes you give each device. The main idea behind it: **a COM port number is not a
device.**

- A device gets a port number from the operating system each time it connects, and that number can
  change.
- ComInspect records the device itself: its USB serial number, Bluetooth address or USB socket.
- It then shows which port the device has right now.

Contents:

1. [The main window](#the-main-window)
2. [Port status](#port-status)
3. [Naming your ports](#naming-your-ports)
4. [Finding out which port is which](#finding-out-which-port-is-which)
5. [How ComInspect recognizes a device](#how-cominspect-recognizes-a-device)
6. [Hidden ports and COM numbers on Windows](#hidden-ports-and-com-numbers-on-windows)
7. [Warnings and notices](#warnings-and-notices)
8. [Diagnostics](#diagnostics)
9. [Search, filters and keyboard shortcuts](#search-filters-and-keyboard-shortcuts)
10. [Moving to another computer: export and import](#moving-to-another-computer-export-and-import)
11. [Your data and backups](#your-data-and-backups)
12. [Updates](#updates)
13. [Troubleshooting](#troubleshooting)

---

## The main window

![Main window](images/main-light.png)

### The port table

The table has one row per device, with these columns:

| Column | Meaning |
|---|---|
| **Status** | Whether the device is connected now (see [Port status](#port-status)) |
| **Port** | Its current or last known port: `COM7` on Windows, `/dev/ttyUSB0` on Linux, `/dev/cu.usbserial-…` on macOS. A small *was COM5* tag appears for a week after a port number changes. |
| **Nickname** | Your name for the port. Double-click it to edit it in place. |
| **Device** | What the operating system reports, such as *Silicon Labs Dual CP2105 USB to UART Bridge*. A grey tag in front shows what ComInspect's device database recognized, such as *Enhanced COM port* or *Icom IC-7300*. |
| **Type** | USB, Bluetooth, Virtual, PCI or Built-in |
| **VID:PID** | The USB vendor and product ID |
| **Serial** | The USB serial number, if the device has one |
| **Purpose** | What you use the port for, set by you. A ✓ after CAT means you marked it *Verified CAT*. |
| **Last Seen** | *Now* while connected; otherwise when it was last connected |

Click a column header to sort by it. The default order puts connected ports first, then sorts by
port number, with COM2 before COM10.

### The rest of the window

- **Details panel (right).** Everything ComInspect knows about the selected port. The toolbar
  button next to **Refresh** shows or hides it.
- **Status bar (bottom).**
  - Counts of connected and disconnected ports.
  - Driver problems and notices. Click either for the full list.
  - On Windows, **COM numbers**, which opens the [COM number map](#the-com-number-map).
  - **Live**, which shows that ComInspect is watching for devices being plugged in or removed.
- **Menu (☰, top right).** About & updates, export and import, database backups, showing ignored
  ports, and opening the data and log folders.

The list updates by itself when a device is plugged in or removed. A short message such as
*FTDX10 CAT Enhanced connected on COM7 (was COM5)* appears in the corner. **Refresh** (F5) rescans
immediately.

---

## Port status

| Symbol | Status | Meaning |
|---|---|---|
| ● green | **Connected** | The device is plugged in and working. |
| ● red + ⚠ | **Connected, with a driver problem** | Windows sees the device but can't use it, for example the Prolific "Code 10". The details panel explains the problem and what to do. |
| ◌ grey, dashed | **Hidden** (Windows only) | The device is unplugged, but Windows still keeps its COM number reserved. Device Manager shows these only under *View → Show hidden devices*. |
| ○ grey | **Disconnected** | ComInspect has seen this device before, but it isn't connected now. |
| ◌ blue, dotted | **Awaiting** | The device came from an [import](#moving-to-another-computer-export-and-import) and hasn't been connected to this computer yet. |

Disconnected devices stay in the list with your names and notes. When a device is plugged in again,
even on a different port, it gets its details back.

---

## Naming your ports

Select a port and fill in **User identity** in the details panel. Changes save automatically.

| Field | What to put there |
|---|---|
| **Nickname** | A short name you will recognize in other programs' port lists, such as *Main rig CAT*, *APRS TNC* or *Rotator*. |
| **Equipment** | The radio, TNC, interface or cable model, such as *Yaesu FTDX10*, *Digirig Mobile* or *Kenwood PG-5G cable*. |
| **Purpose** | One of: CAT, Secondary CAT, PTT, CW, Programming, Data, KISS, GPS, Control, Other or Unknown. |
| **Category** | The kind of device: Radio CAT, PTT, CW Keying, TNC, KISS, GPS, Rotator, Amplifier, Antenna tuner, Antenna switch, Programming cable, Radio interface, Bluetooth serial, Virtual serial, Generic serial or Other. |
| **CAT** | **Verified CAT**, **Not CAT** or **Unknown**. See below. |
| **Notes** | Anything useful, such as which programs use the port, its baud rate, or what RTS and DTR do. |

To rename quickly without the details panel, double-click the nickname in the table, or select the
row and press **F2**. **Enter** saves and **Esc** cancels.

### Purpose and "Verified CAT"

**Purpose** records what you *intend* to use a port for. **CAT** records what you have *confirmed*.

- ComInspect never marks a port as CAT by itself, even when its device database has a strong hint.
- A radio that shows up as two ports has a CAT port and a second port, often used for PTT, CW or
  RTTY keying through RTS/DTR. The device database can tell you which is which. You confirm it by
  checking against the radio's manual, or with a [CAT query](#cat-query-read-only).
- After a successful CAT query, ComInspect offers **Mark as Verified CAT**.

Set **Not CAT** on ports you have checked and ruled out, such as the second port of a dual-port
radio. That helps when you come back months later.

---

## Finding out which port is which

- **Unplug and replug.** The corner message names the port that changed, and **Last Seen** updates.
- **Look at the suggestions.** For recognized hardware, the **Suggestions** section of the details
  panel explains what the port usually is. Examples:
  - the *Enhanced* and *Standard* ports of a Silicon Labs CP2105
  - an Icom radio whose USB serial number contains the model name
  - chips that are commonly counterfeited

  Suggestions are never applied automatically.
- **Read the USB interface.** Dual-port devices show *Interface 0* and *Interface 1* under
  **Hardware**. Most radio manuals say which interface or port is CAT.
- **Ask the radio.** The [read-only CAT query](#cat-query-read-only) sends one harmless command,
  such as "what is your ID?" or "what is your frequency?". A valid reply means you found the CAT
  port and its baud rate.
- **Find the PTT line.** The [PTT test](#ptt-test) briefly keys the transmitter through RTS or DTR.
  Use it only with an antenna or dummy load connected.

---

## How ComInspect recognizes a device

When a device appears, ComInspect matches it with a device it already knows. It uses the most
reliable identifier available:

| Identifier | Used for | Survives moving to another USB socket? |
|---|---|---|
| **USB serial number** (+ VID, PID and interface) | FTDI, most CP210x, radios with built-in USB | Yes |
| **Bluetooth address** | Bluetooth serial ports | Yes |
| **OS instance** | The operating system's own device ID | Usually |
| **USB socket** (+ VID and PID) | Cables without a unique serial number, such as CH340, many PL2303 and some CP210x | No |
| **Virtual driver and port** | com0com, VSPE and similar virtual ports | Not applicable |
| **Port name only** | Last resort, such as the motherboard's COM1 | Not applicable |

**How it is recognized**, in the details panel, shows which identifiers matched.

### Cables without a serial number

Many inexpensive programming cables have no serial number, or share one default number such as
`0001` with every other cable of that type. ComInspect then recognizes the cable by the USB socket
it is plugged into. If you move it to another socket, it shows up as a new device.

To tell ComInspect that two entries are the same cable, select the new entry and click **Same
device as…** at the bottom of the details panel. Then pick the old entry. The entries are linked, and your name,
notes and history are kept. The cable is then recognized in both sockets.

### Ignoring and forgetting

Both buttons are at the bottom of the details panel.

- **Ignore port** hides a port you don't care about, such as the motherboard's COM1 or an Intel
  AMT "Serial over LAN" port. **Menu → Show ignored ports** brings them back.
- **Forget…** removes a device and everything ComInspect recorded about it. If it is still
  connected, it comes back as a new device on the next scan.

---

## Hidden ports and COM numbers on Windows

Windows gives each serial device a COM number and keeps that number reserved after the device is
unplugged. Plug in several cables over the years and new devices end up on COM15, COM23 and higher,
even though only two are connected.

ComInspect shows:

- **Hidden devices.** Unplugged devices that still hold a COM number (status: dashed circle).
  These are the devices Device Manager lists under *Show hidden devices*.
- **Reserved numbers.** COM numbers marked as used in the Windows COM port database.
- **Stale reservations.** Numbers reserved by a device that no longer exists at all.
- **Duplicates.** Two devices set to the same COM number.

### The COM number map

![COM number map](images/com-map.png)

Click **COM numbers** in the status bar to see which of COM1–COM256 are in one of these states:

- used by a connected device
- held by a hidden device
- reserved without a device
- free

### Freeing COM numbers

ComInspect shows problems but doesn't change Windows settings. To free numbers today:

1. In Device Manager choose **View → Show hidden devices**, expand **Ports (COM & LPT)**, then
   right-click a greyed-out device you no longer use and choose **Uninstall device**.
2. To change a connected device's number, open its **Properties → Port Settings → Advanced → COM
   Port Number**. Windows may label a number *(in use)* when only a hidden device holds it.
3. After the cleanup, ComInspect updates the list by itself.

If a device later reappears with a new number, ComInspect still recognizes it and keeps its name.

---

## Warnings and notices

Findings appear in the status bar and in the details panel of the affected port.

| Finding | What it means | What to do |
|---|---|---|
| **Driver problem** (error) | Windows can't start the device. The text includes the Windows problem code. | Follow the advice shown. For **Prolific PL2303 with Code 10**, the chip is almost always counterfeit or discontinued. Either use an older Prolific driver that still accepts it, or replace the cable with an FTDI- or CP210x-based one. |
| **COMx is assigned to … devices** | Two devices use the same COM number. | If both are connected, give one of them a free number. If the other one is hidden, uninstall the hidden device. |
| **Serial number … is not unique** (info) | The device reports a default serial number shared by many devices. | Keep the device in the same USB socket, or use **Same device as…** after moving it. |
| **… report the same USB serial number** | Several connected devices report the same serial number. | Keep each one in its own USB socket. |
| **No permission to open …** (Linux/macOS) | Your user account can't open the port. | Add yourself to the group shown, usually `dialout`, then log out and back in. |
| **COM numbers reserved without a device** (info) | Stale reservations left by uninstalled devices. | Nothing is required. They push new devices to higher numbers. |
| **COM numbers held by disconnected devices** (info) | Hidden devices keep numbers reserved. | Uninstall the hidden devices you no longer use. |
| **COMx is a high port number** (info) | Some older programs can only open COM1–COM9, or list only COM1–COM16. | If a program can't see the port, change the port to a lower free number (see above). |

---

## Diagnostics

![Diagnostics](images/diagnostics.png)

Diagnostics are the only part of ComInspect that opens a serial port, and only when you click a
button. Read this section before using them.

> **Why this matters.** Many stations use a port's RTS or DTR line to key PTT or CW. Opening a port
> can briefly change those lines; Linux, for example, raises DTR and RTS when a port is opened.
> ComInspect releases both lines immediately after opening, but the operating system may pulse them
> for a moment. Keep that in mind on ports wired to a transmitter.

Diagnostics are available only while the device is connected. They include:

- the port availability test
- the read-only CAT query
- the PTT test

### Port availability

**Test open** opens and closes the port without sending anything. It reports:

- **Opened.** The port works. The states of the incoming control lines (CTS, DSR, DCD and RI) are
  also shown.
- **In use.** Another program has the port open. On Linux, ComInspect names the program when it
  can.
- **Permission denied**, **Not found** or another error, with an explanation.

### CAT query (read-only)

The CAT query sends **one read-only command** and shows the reply:

| Command | For radios that use |
|---|---|
| ASCII CAT: read radio ID (`ID;`) | Kenwood, Elecraft, most modern Yaesu, FlexRadio and many SDR programs |
| ASCII CAT: read VFO A frequency (`FA;`) | The same radios, if they don't answer `ID;` |
| CI-V: read transceiver ID | Icom, and radios that follow CI-V (for example Xiegu) |
| CI-V: read operating frequency | The same radios |
| Legacy Yaesu 5-byte CAT: read frequency and mode | FT-817, FT-818, FT-857, FT-897 and similar |

- **Starting command.** ComInspect picks a starting command from what the device reports about
  itself. You can change every setting.
- **Baud.** **Auto** tries the common rates for the chosen protocol in turn and reports the one that
  worked. After a valid reply, click **Use … baud for the next query** to lock it in.
- **CI-V address.** Use the radio's address in hex, from the radio's CI-V menu. Left blank, the
  query goes to address `00`, which many Icom radios answer.
- **Results.**
  - A **Reply received** result means the radio answered. Where the reply identifies the model,
    it is shown, for example *ID 0761 (listed as Yaesu FTDX10)*.
  - You can then click **Mark as Verified CAT**.
  - **No valid reply** means nothing sensible came back. Check that the radio is on, the port and
    baud rate are right, and another program such as your logger or rig control isn't holding the
    port.

The raw bytes sent and received are shown in hex, for troubleshooting.

These queries only *read* information and never change the radio's settings. ComInspect never
sends them automatically.

### PTT test

The PTT test asserts **RTS** or **DTR** for 0.5, 1 or 2 seconds, then releases it. Use it to find
the line that keys your transmitter.

> **Your radio will transmit** if the line controls PTT. Connect an antenna or dummy load, choose a
> frequency where you may transmit, and stay within your license privileges. ComInspect asks for
> confirmation every time.

---

## Search, filters and keyboard shortcuts

- **Search** (Ctrl+F, or ⌘F on a Mac) looks in:
  - nicknames, ports and equipment
  - manufacturer and product names
  - VID:PID and serial numbers
  - notes
- **Filter chips** below the toolbar:
  - status: Connected, Disconnected
  - type: USB, Bluetooth, Virtual
  - purpose: CAT, PTT, KISS, Unknown

  Chips in the same group widen the results, so *USB* and *Bluetooth* shows both. Chips in
  different groups narrow them, so *Connected* and *CAT* shows only connected CAT ports. **Clear
  filters** or **Esc** resets them.

| Key | Action |
|---|---|
| ↑ / ↓ | Select the previous or next port |
| Enter | Edit the selected port's nickname in the details panel |
| F2 | Rename the selected port in the table |
| Ctrl+F / ⌘F | Search |
| F5, Ctrl+R / ⌘R | Rescan |
| Esc | Clear filters, or deselect the port |

---

## Moving to another computer: export and import

**Menu → Export port mappings…** saves a `serial-port-inventory.json` file. Every device in it has
three separate parts:

- **Device identity:** the USB serial number, VID:PID, Bluetooth address and other identifiers.
- **Your labels:** nickname, equipment, purpose, category, CAT status and notes.
- **This computer's assignment:** the COM port it had here and when it was last seen.

On another computer, **Menu → Import port mappings…** reads the file:

- **Matching devices.** Devices with a unique identity, such as a USB serial number or Bluetooth
  address, get your names as soon as they are plugged in, whatever port they receive there. Devices
  that aren't connected yet are listed as **Awaiting** until they are.
- **Cables without a unique serial number** are only recognized on the computer where they were
  exported. On the new computer, name them again or use **Same device as…**.
- **Merge** keeps your existing names and notes on this computer and fills in only what's missing.
  **Replace** uses the names, purposes and notes from the file wherever it has them.
- **Safety.** A database backup is taken before every import.

The export file contains no personal data beyond what you typed into ComInspect and the computer's
name. Review it before sharing it.

---

## Your data and backups

Everything is stored locally in one SQLite database:

| System | Data folder | Log folder |
|---|---|---|
| Windows | `%LOCALAPPDATA%\io.github.kk4oda.cominspect\` | `%LOCALAPPDATA%\io.github.kk4oda.cominspect\logs\` |
| macOS | `~/Library/Application Support/io.github.kk4oda.cominspect/` | `~/Library/Logs/io.github.kk4oda.cominspect/` |
| Linux | `~/.local/share/io.github.kk4oda.cominspect/` | `~/.local/share/io.github.kk4oda.cominspect/logs/` |

**Menu → Open data folder** and **Open log folder** open them.

### Backups

- **When they are taken.** ComInspect writes a backup to the `backups` folder:
  - before every database upgrade
  - before every app update
  - before every import
  - whenever you click **Back up now** in **Menu → Database backups…**
- **How many are kept.** The ten newest backups.
- **Restoring.** **Menu → Database backups…** lists the backups. **Restore…** replaces the current
  database with the backup you choose. The current database is moved aside, next to the original
  file, not deleted.

### If something goes wrong with the database

- **The database file is damaged.** ComInspect moves it aside, starts a fresh one and tells you
  where the damaged file is.
- **An older version opens a newer database.** This can happen after going back to an older
  version. ComInspect leaves the file untouched and runs on a temporary database, and a banner
  says so. Update ComInspect again, or restore the backup taken before the upgrade.

---

## Updates

![About and updates](images/updates.png)

- **Checking.** ComInspect checks for a new version once a day, in the background. Turn this off
  under **Menu → About & updates**.
- **When an update is available.** A dot appears on the menu button, and **About & updates** shows:
  - the installed and latest versions
  - the release date and release notes
  - **Download and install update**
- **Installing.** Nothing installs until you click that button. The download is verified with the
  project's signing key before anything is replaced. Then ComInspect restarts.
- **Check for updates** checks immediately. When there is nothing new, it says *You're up to date.*
- **Linux packages.** On a `.deb` or `.rpm` installation, you'll be asked for your password to
  install the new package. If a release has no in-app update for your package type, ComInspect says
  so and links to the download page instead.

### Channels

**About & updates → Advanced → Update channel**:

- **Stable** (default) receives only full releases.
- **Beta** receives pre-release versions for testing, plus every newer stable release. You never
  receive beta versions unless you choose this. You can switch back at any time, and you'll then
  move to the next stable release that is newer than your beta.

### If an update goes wrong

- If a new version fails to start properly, the next launch shows a banner offering the log
  folder and **Reinstall previous version**.
- The same button is under **About & updates → Advanced** after any update.
- Reinstalling uses the previous release's signed files, and your data is kept.

---

## Troubleshooting

**A port doesn't appear at all.**

- Check that the device's driver is installed. Windows Device Manager should list it under
  *Ports (COM & LPT)*.
- Try another cable or USB socket. Some USB cables carry power only.
- **Linux, CH340/CH341 devices (VID:PID `1A86:7523`).** The `brltty` braille-display service on
  some distributions claims these devices, and the port disappears a second after it's plugged in.
  Remove it with `sudo apt remove brltty` if you don't use a braille display.

**"No permission" on Linux.**

- Run `sudo usermod -aG dialout $USER` (on Arch Linux, use the group `uucp`), then log out and back
  in.
- `ls -l /dev/ttyUSB0` shows which group owns a port.

**A device misbehaves right after it's plugged in (Linux).**

- ModemManager may be sending modem commands to new `ttyACM` ports.
- Disable it (`sudo systemctl disable --now ModemManager`) if you don't use a mobile-broadband
  modem.
- Or add a udev rule with `ENV{ID_MM_DEVICE_IGNORE}="1"` for the device.

**Another program says the port is busy.**

- Only one program can use a serial port at a time.
- **Test open** shows whether the port is in use and, on Linux, which program has it.
- Close that program, or share the radio through rig-control software such as flrig, Hamlib's
  rigctld or OmniRig.

**Prolific cable with a yellow warning (Code 10) on Windows.**

See the **Driver problem** row under [Warnings and notices](#warnings-and-notices).

**macOS: which device file?**

- ComInspect lists the `/dev/cu.*` "call-out" device, which is the one to use in radio software.
- The matching `/dev/tty.*` device waits for a carrier signal and can hang on open.

**Bluetooth serial ports.**

- Windows often creates two COM ports for a paired Bluetooth serial device:
  - an *outgoing* port, which your program opens to connect to the device
  - an *incoming* port, which waits for the device to connect
- The details panel labels each port **Outgoing** or **Incoming**.
- For outgoing ports, it also shows the device's Bluetooth address.
- Radio and TNC software almost always needs the outgoing port.

**Collecting information for a bug report.**

- **Menu → Open log folder** has `cominspect.log`.
- For more detail, start ComInspect with the environment variable `COMINSPECT_LOG=debug`.
- `cominspect-cli list --json` (see the [README](../README.md#command-line-tool)) prints everything
  ComInspect discovers.
- Attach both to an [issue](https://github.com/KK4ODA/ComInspect/issues).
