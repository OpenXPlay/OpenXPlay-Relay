<p align="center"><b>English</b> · <a href="README-zh.md">简体中文</a></p>

# OpenXPlay Relay

The relay of [OpenXPlay](https://github.com/OpenXPlay/OpenXPlay): it connects to the iPhone as a CarPlay receiver and passes the session on to the head unit through [OpenXPlay-Stream](https://github.com/OpenXPlay/OpenXPlay-Stream) outputs, such as the head unit's browser or its Carlink app. Nothing is installed on the head unit.

The relay runs on two platforms:

- **Android app.** It runs on a phone and uses [OpenXPlay-DiPlay](https://github.com/OpenXPlay/OpenXPlay-DiPlay) as its CarPlay receiver and the Kotlin Stream outputs.
- **Linux relay.** It runs on small Linux devices such as the UFI003 USB Wi-Fi dongle. The CarPlay receiver is written in Rust, and the Rust Stream outputs run as a separate `oxp-relay` process. Device support files live in [OpenXPlay-Hardware](https://github.com/OpenXPlay/OpenXPlay-Hardware).

Both platforms serve the same web admin page on port 6975, for Wi-Fi setup, iPhone pairing, outputs and Carlink pairing.

## Download

- **Android app:** [openxplay-relay.apk](https://github.com/OpenXPlay/OpenXPlay-Relay/releases/latest/download/openxplay-relay.apk) (latest release)
- **Linux relay** for aarch64 devices, and every version: [Releases](https://github.com/OpenXPlay/OpenXPlay-Relay/releases)

Setup: [openxplay.org/en/quick-start](https://openxplay.org/en/quick-start).

## Layout

| Directory | Language | Contents |
| --- | --- | --- |
| [`android/`](android) | Kotlin (Gradle) | The Android app (`app/`) and its build logic (`buildSrc/`) |
| [`rust/`](rust) | Rust (Cargo) | The Linux relay: `direlay-linux` (the `openxplay-relay` binary), `carplay-iap2` and `carplay-airplay` (CarPlay protocol crates), `vendor/` (patched dependencies), `packaging/` (systemd unit and installer), `scripts/` |
| [`admin-web/`](admin-web) | TypeScript (Vite) | The admin page shared by both platforms |

## Build and test

The Android build includes DiPlay and Stream as Gradle composite builds. By default it expects them checked out next to this repository (`../OpenXPlay-DiPlay`, `../OpenXPlay-Stream`); pass `-Pdiplay.dir=PATH` or `-Pstream.dir=PATH` (pointing at Stream's `kotlin/` directory) to use other locations.

| Part | Requirements | Commands |
| --- | --- | --- |
| Admin page | Node.js 20.19+ or 22.12+ | `cd admin-web && npm ci && npm test && npm run build` |
| Android app | JDK 17, Android SDK; the admin page and Stream's `web/dist` built first | `cd android && ./gradlew testDebugUnitTest assembleDebug` |
| Linux relay | Rust stable | `cd rust && cargo test` |
| Linux relay for aarch64 | `rustup target add aarch64-unknown-linux-musl`, `llvm-tools`, clang | `rust/scripts/build-aarch64.sh` |

## Contributing

Contributions are accepted under the [Contributor License Agreement](./CLA.md). The CLA Assistant
check on each pull request records your acceptance.

## License

This project is licensed under the [GNU Affero General Public License v3.0 (AGPLv3)](./LICENSE),
version 3 only.

Additional terms under AGPLv3 Section 7 apply to the material contributed by this project. Modified
versions must preserve the attribution notice `Powered by OpenXPlay.` in the appropriate legal
notices and in any prominent about, legal, footer, or attribution location presented by the user
interface, together with a visible link to the original project:
<https://github.com/OpenXPlay/OpenXPlay-Relay>. See [NOTICE](./NOTICE).

The CarPlay protocol crates `rust/carplay-iap2` and `rust/carplay-airplay`, and a few files in
`rust/direlay-linux/src/network/`, are derived from DiPlay and xcertplay. They remain under the
[GNU General Public License v3.0](./LICENSE-GPL-3.0) only, and GPLv3 Section 13 permits combining
them with this AGPLv3 project. Builds also include OpenXPlay-Stream (AGPLv3), so the relay offers
its source to everyone who uses it over a network, including through the head-unit browser.

If your organization's policies do not permit the use of AGPLv3-licensed software, or if you wish
to avoid the open-source obligations of AGPLv3, please contact us at:
[support@openxplay.org](mailto:support@openxplay.org). Exemptions cover the code OpenXPlay owns;
the parts derived from DiPlay and xcertplay stay under GPLv3.

CarPlay and iPhone are trademarks of Apple Inc.; OpenXPlay is an independent project and is not
affiliated with, endorsed or certified by Apple. This repository and its builds contain no Apple
accessory (MFi) identity; a working setup needs one supplied by the user.
