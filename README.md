# Network Overlay

`Network Overlay` is a small Windows desktop overlay built with Rust.

It monitors:
- general network reachability
- website reachability for configured sites
- latency, packet loss, speed label, and trend

The app is designed to stay on screen like a lightweight monitor overlay.

## Features

- Always-on-top transparent overlay
- Hover fade when not in use
- Minimize and restore from the overlay itself
- Close button on the overlay
- Scrollable content when the list is taller than the window
- Native Windows ICMP ping for network checks
- TCP-based website reachability checks for web targets
- Embedded Windows icon and executable metadata using `winres`

## Requirements

- Windows
- Rust toolchain with Cargo installed

## Run

```powershell
cargo run
```

## Build

```powershell
cargo build
```

The built executable will be located at:

```text
target\debug\internet-mon-jaybien.exe
```

## Overlay Controls

- `F8`: Toggle click-through mode
- `F9`: Toggle compact mode
- `-`: Minimize the overlay
- `+`: Restore the overlay
- `X`: Close the overlay

## How Monitoring Works

### Network status

The general network check uses native Windows ICMP echo requests from Rust with:

- `500 ms` timeout
- `32-byte` payload
- `TTL 64`

The network target is currently:

- `8.8.8.8` (`Google DNS`)

### Website status

Website checks do not rely on ICMP ping alone.

Instead, the app:
- parses the site URL
- gets the correct port from the URL
- measures a timed TCP connection to that host and port

This is more accurate for websites that block ping but still work normally over `HTTP` or `HTTPS`.

## Config

The current site list and thresholds are defined in [src/main.rs](C:/Users/giyut/Documents/ProjectsForOtherPeeps/Jaybien%20OJT/internet-mon-jaybien/src/main.rs:1).

You can change:
- monitored sites
- ping target endpoints
- refresh interval
- latency thresholds
- overlay dimensions

## Assets

App icon files are stored in:

- [assets/app-icon.png](C:/Users/giyut/Documents/ProjectsForOtherPeeps/Jaybien%20OJT/internet-mon-jaybien/assets/app-icon.png)
- [assets/app-icon.ico](C:/Users/giyut/Documents/ProjectsForOtherPeeps/Jaybien%20OJT/internet-mon-jaybien/assets/app-icon.ico)

Windows executable metadata and icon embedding are configured in [build.rs](C:/Users/giyut/Documents/ProjectsForOtherPeeps/Jaybien%20OJT/internet-mon-jaybien/build.rs:1).

## Notes

- This project is currently Windows-focused.
- The overlay uses native Windows APIs for ICMP monitoring.
- For websites, TCP reachability is used because some sites block ICMP while still being online.
