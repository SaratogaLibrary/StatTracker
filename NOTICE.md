# Third-Party Notices

StatTracker itself is licensed under the MIT License. See [LICENSE.md](LICENSE.md).

This desktop app is built with Tauri (Rust + a system webview) and ships
third-party code under their own licenses. Those licenses remain in force for
that code. This file summarizes what is included; it is not legal advice.

A full crate graph is in `src-tauri/Cargo.lock`. You can regenerate a license
list from that lockfile (for example with `cargo license` in `src-tauri`).

## Direct Rust dependencies

These crates are declared in `src-tauri/Cargo.toml`:

| Crate | License |
| --- | --- |
| tauri | Apache-2.0 OR MIT |
| tauri-build | Apache-2.0 OR MIT |
| tauri-plugin-autostart | MIT OR Apache-2.0 |
| serde | MIT OR Apache-2.0 |
| serde_json | MIT OR Apache-2.0 |
| toml | MIT OR Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| rusqlite | MIT |
| chrono | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| auto-launch | MIT |
| directories | MIT OR Apache-2.0 |
| open | MIT |
| url | MIT OR Apache-2.0 |

Most transitive crates in the lockfile are also MIT, Apache-2.0, or dual-licensed
MIT OR Apache-2.0. The remainder of this notice calls out licenses that are
not that dual grant, plus platform libraries that are not Cargo crates.

## Mozilla Public License 2.0

These crates (pulled in through Tauri / wry HTML and CSS parsing) are MPL-2.0.
MPL-2.0 is a file-level copyleft: modified MPL files must stay MPL-2.0, and
source for those files must be available. Linking them into this MIT app does
not relicense StatTracker as a whole.

- cssparser
- cssparser-macros
- dtoa-short
- markup5ever
- selectors

Source for these crates is published on [crates.io](https://crates.io/) and
[GitHub](https://github.com/).

## Apache-2.0 (including LLVM exception)

Some crates are Apache-2.0 only, or Apache-2.0 WITH LLVM-exception. Notable
examples in this lockfile:

- wry (Apache-2.0 WITH LLVM-exception)
- json-patch, jsonptr
- rustc_version
- schannel
- vswhom, vswhom-sys
- selected `windows-*` / `windows_x86_64_msvc` packages
- selected `hashbrown` / `equivalent` versions

Apache-2.0 requires preserving copyright, patent, and NOTICE attributions for
that code. The LLVM exception (on wry) relaxes some copyleft-style conditions
when the code is compiled into an executable.

## Unicode License v3

Internationalization data and helpers from the ICU4X / `icu_*` crates, plus
related types (`tinystr`, `yoke`, `zerovec`, and similar), are licensed under
Unicode-3.0 (some also dual-licensed CC0-1.0). Unicode data files and software
must keep the Unicode copyright and permission notice.

## Other permissive crate licenses

| License | Examples in this lockfile |
| --- | --- |
| BSD-3-Clause | encoding_rs, instant, subtle |
| ISC | native-tls, rfd, simple_asn1; libsqlite3-sys is MIT OR Apache-2.0 OR ISC |
| Boost Software License 1.0 | ryu |
| CC0-1.0 | foldhash (also MIT OR Apache-2.0); some ICU helper crates (also Unicode-3.0) |
| Zlib | foldhash (also MIT OR Apache-2.0) |
| Unlicense OR MIT | byteorder |
| MIT AND BSD-2-Clause | webpki-roots |

## SQLite

`rusqlite` / `libsqlite3-sys` may statically link the SQLite amalgamation.
SQLite itself is public domain. The Rust bindings remain under the licenses
listed above.

## npm toolchain (not shipped in the app)

These packages are used to build the Tauri bundle. They are not part of the
runtime widget:

| Package | License |
| --- | --- |
| @tauri-apps/cli | Apache-2.0 OR MIT |
| semver | ISC |

## Platform webviews and system libraries

Tauri renders UI in the OS webview. Those components are not redistributed as
source in this repository:

- **Windows:** WebView2 (Microsoft Edge WebView2 Runtime). Redistribution
  follows Microsoft’s WebView2 terms.
- **macOS:** WKWebView (Apple system framework).
- **Linux:** WebKitGTK, GTK, and related libraries are typically LGPL
  system libraries provided by the distribution. If you ship a Linux
  binary that dynamically links them, keep them as separate system
  libraries and follow the LGPL’s linking and source-offer rules for those
  libraries. This repository does not vendor WebKitGTK or GTK.

## What this notice does not cover

The backend HTTP API this widget talks to is a separate
application. It is not a compile-time dependency of StatTracker and is not
covered by this notice.
