# Kauwlendar

> **Visual, high-contrast calendar dashboard for jailbroken Amazon Kindles**, built with Rust for E-Ink displays.

Kauwlendar turns your Kindle into an elegant, low-power, high-contrast daily agenda display. It fetches live events from **Google Calendar, Outlook, or Apple Calendar** (via private iCal/ICS URLs), renders a modern card-based interface optimized for E-Ink screens, and allows a clean return to the Kindle OS at any time with a single tap on the screen.

---

## Features

* **Big Agenda Cards design:**
  * High-contrast dark header featuring a large clock (92px), localized date in Spanish, and battery level pill badge.
  * Enlarged event cards (195px–240px) with 48px bold titles, time range pills (`09:00 AM - 10:30 AM` or `Todo el día`), and calendar category pills (`Personal`, `Trabajo`).
  * Minimalist footer with no visual clutter.
* **KPM / Scriptlets integration (Library book cover):**
  * Launches like a native book directly from the Kindle library using `Kauwlendar.sh` in `/mnt/us/documents/` and a custom book cover `icon.png` (250x391 px).
  * No need to open KUAL manually: start it with a single tap from the home screen.
* **Live synchronization without stale cache:**
  * Downloads and processes calendars on launch and once per local day transition.
* **Multiple calendars and recurring events (RRULE):**
  * Consolidates Google, Outlook, or Apple calendars into a unified chronological agenda.
  * Supports daily, weekly, monthly, and yearly recurring events, respecting exceptions and cancellations (`EXDATE`).
* **Configurable 12h (AM/PM) or 24h format:**
  * Customize time formatting in `calendars.toml`.
* **Multi-language support (English and Spanish):**
  * Decoupled i18n architecture supporting Spanish (`language = "en"`) and English (`language = "en"`) for all interface texts, date headers, buttons, and error screens.
* **Interactive exit button (`[ SALIR ]` / `[ EXIT ]`):**
  * Multi-device touch input detection (`/dev/input/event*`) in the top-right corner.
  * Pauses and resumes the Kindle graphical environment (`cvm`, `awesome`, `mesquite`) cleanly without restarting the UI framework or freezing the display.
* **Smart Wi-Fi battery optimization:**
  * Connects only when entering the app and during morning daily refresh (6:00 AM). Stays off throughout the day using in-memory cached events to preserve battery life.
  * Restores the device's original Wi-Fi state upon exit.
* **Fast compilation without Docker:**
  * Includes `./build-kindle.sh` using Clang and LLVM to produce a lightweight static ARMv7 musl binary in seconds.
* **Local PC development and preview:**
  * Run locally with `cargo run -- --oneshot`.
  * Automatically generates `preview.png` matching the exact E-Ink layout and resolution of the Kindle.

---

## Layout Preview

When running, Kauwlendar renders a high-contrast layout tailored to the native resolution of the Kindle (1072 x 1448 px on Paperwhite, adaptive to other models):

```text
┌────────────────────────────────────────────────────────────────────────┐
│  Monday, November 09, 2026                    [85%]        [ EXIT ]    │
│  08:30 AM                                                              │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  ╭──────────────────────────────────────────────────────────────────╮  │
│  │ [ All Day ]  [ Personal ]                                        │  │
│  │ All-Day Event                                                    │  │
│  ╰──────────────────────────────────────────────────────────────────╯  │
│                                                                        │
│  ╭──────────────────────────────────────────────────────────────────╮  │
│  │ [ 09:00 AM - 10:30 AM ]  [ Work ]                                │  │
│  │ Event 1                                                          │  │
│  ╰──────────────────────────────────────────────────────────────────╯  │
│                                                                        │
│  ╭──────────────────────────────────────────────────────────────────╮  │
│  │ [ 02:15 PM - 03:00 PM ]  [ Personal ]                            │  │
│  │ Event 2                                                          │  │
│  ╰──────────────────────────────────────────────────────────────────╯  │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│                    Tap [EXIT] to return to Kindle                      │
└────────────────────────────────────────────────────────────────────────┘
```

---

## Prerequisites

* **Jailbroken Kindle** (compatible with Paperwhite, Oasis, Basic, and Voyage running firmware 5.x).
* **KPM (Kindle Package Manager) with Scriptlets / `SH_Integration`** (Recommended) or **KUAL**.
* **FBInk** installed (bundled with KOReader at `/mnt/us/koreader/fbink` or installed as a standalone extension at `/usr/bin/fbink`).

---

## Building for Kindle (ARM)

The binary must be statically compiled for the Kindle's ARMv7 architecture (`armv7-unknown-linux-musleabi`).

### Method 1: Native script with Clang / LLVM (Recommended — Fast, no Docker)

This method compiles the binary in seconds using your system's LLVM toolchain.

#### 1. Install prerequisites (Debian / Ubuntu):
```bash
sudo apt update
sudo apt install clang llvm lld
rustup target add armv7-unknown-linux-musleabi
```

#### 2. Build:
```bash
./build-kindle.sh
```

The script invokes `clang`, `llvm-ar`, and `rust-lld` configured in `.cargo/config.toml` to produce a standalone static binary at:
```text
target/armv7-unknown-linux-musleabi/release/kauwlendar
```

---

### Method 2: Using `cross` (Docker alternative)

If you prefer using a containerized environment:

```bash
cargo install cross
cross build --target armv7-unknown-linux-musleabi --release
```

The resulting binary will be located at the same path:
```text
target/armv7-unknown-linux-musleabi/release/kauwlendar
```

---

## Installation on Kindle

### 1. Create the application folder on the Kindle
Connect your Kindle via USB and create the `kauwlendar` folder at the root of the user storage:
```text
/mnt/us/kauwlendar/
```

### 2. Copy the files

Copy the following 4 files to their respective locations on the Kindle:

| Source File | Destination on Kindle | Description |
|-------------|-----------------------|-------------|
| `target/armv7-unknown-linux-musleabi/release/kauwlendar` | `/mnt/us/kauwlendar/kauwlendar` | Compiled static executable |
| `calendars.toml` | `/mnt/us/kauwlendar/calendars.toml` | Configuration file with calendar URLs and options |
| `icon.png` | `/mnt/us/kauwlendar/icon.png` | Library book cover image (250x391 px) |
| `Kauwlendar.sh` | `/mnt/us/documents/Kauwlendar.sh` | Launcher script for KPM / Scriptlets |

> Note: Why place `Kauwlendar.sh` in `/mnt/us/documents/`?  
> KPM with `SH_Integration` automatically indexes shell scripts located in `documents/` as books in the Kindle library. Using the metadata headers in `Kauwlendar.sh` and the cover image at `/mnt/us/kauwlendar/icon.png`, Kauwlendar appears directly in your Kindle home screen with a custom cover.

### 3. Set execution permissions (via SSH)
If connecting via SSH, ensure execution permissions are set:
```bash
chmod +x /mnt/us/kauwlendar/kauwlendar
chmod +x /mnt/us/documents/Kauwlendar.sh
```

---

## Usage on Kindle

1. **Launch:** From the Kindle home screen or library, tap the **Kauwlendar** book cover (or run it via KUAL / SSH).
2. **View:** Kauwlendar will fetch today's events and refresh the E-Ink display with the card layout.
3. **Exit:** Tap the **`[ SALIR ]`** button in the top-right corner of the screen. The native Kindle interface will resume immediately.

---

## Configuration (`calendars.toml`)

Create or edit `/mnt/us/kauwlendar/calendars.toml` (or in the repository root for local testing):

```toml
# Refresh interval in minutes during active hours (minimum 5)
refresh_minutes = 60

# UTC timezone offset in hours (e.g., 0 for UTC, -4 for UTC-4, -5 for UTC-5)
utc_offset_hours = 0

# Time display format:
# "12h" -> AM/PM format (e.g. 09:30 AM, 02:15 PM)
# "24h" -> 24-hour format (e.g. 09:30, 14:15)
time_format = "12h"

# Interface language:
# "en" -> English (default): [ EXIT ], "Monday, November 09, 2026", "All Day"
# "es" -> Spanish: [ SALIR ], "Lunes, 09 de Noviembre de 2026", "Todo el día"
language = "en"

# Calendars to synchronize (supports multiple entries)
[[calendars]]
name = "Personal"
url = "https://calendar.google.com/calendar/ical/YOUR_EMAIL/private-xxxx/basic.ics"

[[calendars]]
name = "Work"
url = "https://outlook.office365.com/owa/calendar/xxxx/reachcalendar.ics"
```

### Obtaining your private iCal URL from Google Calendar
1. Open [Google Calendar](https://calendar.google.com/) in your browser.
2. In the left sidebar, find the calendar you want to display, click the three dots `⋮`, and select **Settings and sharing**.
3. Scroll down to the **Integrate calendar** section.
4. Copy the URL labeled **"Secret address in iCal format"**.
   > Important: Always use the secret address so Kauwlendar can access your private events without requiring interactive login.

---

## Local Development and PC Testing

You can test and verify everything on your computer without connecting a Kindle:

1. **Clone the repository:**
   ```bash
   git clone git@github.com:arisness/kauwlendar.git
   cd kauwlendar
   ```

2. **Create a local configuration:**
   ```bash
   cp calendars.toml.example calendars.toml
   ```

3. **Run a single-shot test:**
   ```bash
   cargo run -- --oneshot
   ```
   * Terminal output will display the loaded events.
   * A `preview.png` file will be generated in the project root.
   * Open `preview.png` with an image viewer to verify the exact E-Ink visual layout.

4. **Run unit tests:**
   ```bash
   cargo test
   ```

---

## License

This project is licensed under the MIT License. Feel free to modify and adapt it for your device.
