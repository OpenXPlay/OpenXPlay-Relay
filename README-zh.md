<p align="center"><a href="README.md">English</a> · <b>简体中文</b></p>

# OpenXPlay Relay

[OpenXPlay](https://github.com/OpenXPlay/OpenXPlay) 的中继：它作为 CarPlay 接收端连接 iPhone，再通过 [OpenXPlay-Stream](https://github.com/OpenXPlay/OpenXPlay-Stream) 的输出把会话转给车机，例如车机的浏览器或 Carlink 应用。车机无需安装任何软件。

中继可在两种平台上运行：

- **Android 应用。** 运行在手机上，使用 [OpenXPlay-DiPlay](https://github.com/OpenXPlay/OpenXPlay-DiPlay) 作为 CarPlay 接收端，并使用 Kotlin 版 Stream 输出。
- **Linux 中继。** 运行在小型 Linux 设备上，例如 UFI003 USB 随身 Wi-Fi。CarPlay 接收端用 Rust 编写，Rust 版 Stream 输出作为独立的 `oxp-relay` 进程运行。设备支持文件位于 [OpenXPlay-Hardware](https://github.com/OpenXPlay/OpenXPlay-Hardware)。

两种平台都在 6975 端口提供同一个网页管理页面，用于 Wi-Fi 设置、iPhone 配对、输出配置和 Carlink 配对。

## 下载

- **Android 应用：**[openxplay-relay.apk](https://github.com/OpenXPlay/OpenXPlay-Relay/releases/latest/download/openxplay-relay.apk)（最新版本）
- **Linux 中继**（aarch64 设备）及所有版本：[Releases](https://github.com/OpenXPlay/OpenXPlay-Relay/releases)

设置方法见 [openxplay.org/quick-start](https://openxplay.org/quick-start)。

## 目录结构

| 目录 | 语言 | 内容 |
| --- | --- | --- |
| [`android/`](android) | Kotlin（Gradle） | Android 应用（`app/`）及其构建逻辑（`buildSrc/`） |
| [`rust/`](rust) | Rust（Cargo） | Linux 中继：`direlay-linux`（`openxplay-relay` 可执行文件）、`carplay-iap2` 和 `carplay-airplay`（CarPlay 协议 crate）、`vendor/`（打过补丁的依赖）、`packaging/`（systemd unit 和安装程序）、`scripts/` |
| [`admin-web/`](admin-web) | TypeScript（Vite） | 两种平台共用的管理页面 |

## 构建与测试

Android 构建以 Gradle 复合构建（composite build）的方式引入 DiPlay 和 Stream。默认要求二者检出在本仓库旁边（`../OpenXPlay-DiPlay`、`../OpenXPlay-Stream`）；如需使用其他位置，请传入 `-Pdiplay.dir=PATH` 或 `-Pstream.dir=PATH`（指向 Stream 的 `kotlin/` 目录）。

| 部分 | 要求 | 命令 |
| --- | --- | --- |
| 管理页面 | Node.js 20.19+ 或 22.12+ | `cd admin-web && npm ci && npm test && npm run build` |
| Android 应用 | JDK 17、Android SDK；需先构建管理页面和 Stream 的 `kotlin/web-receiver/dist` | `cd android && ./gradlew testDebugUnitTest assembleDebug` |
| Linux 中继 | Rust 稳定版 | `cd rust && cargo test` |
| 面向 aarch64 的 Linux 中继 | `rustup target add aarch64-unknown-linux-musl`、`llvm-tools`、clang | `rust/scripts/build-aarch64.sh` |

所有构建都使用 `stream.lock` 和 `diplay.lock` 中记录的 Stream 与 DiPlay 提交；本地检出不是该提交或有未提交改动时，Android 构建会给出警告（加 `-Prelay.strictSources=true` 则直接报错）。要升级锁定的提交，请先把新提交推送到该仓库的 `main`，再运行 `scripts/sources.sh bump stream`（或 `diplay`）并提交锁文件。

## 参与贡献

贡献须遵循 [OpenXPlay 贡献者许可协议](https://github.com/OpenXPlay/OpenXPlay/blob/main/CLA.md)（协议文本为英文）。在任意一个 OpenXPlay 仓库的 pull request 里按提示回复一次即可，之后所有仓库的 CLA 检查都会通过。

## 许可证

本项目采用 [GNU Affero General Public License v3.0 (AGPLv3)](./LICENSE) 授权，仅限第 3 版。

本项目贡献的内容适用依据 AGPLv3 第 7 条设定的附加条款。修改版本必须在相应的法律声明中，以及用户界面中任何显著的“关于”、法律、页脚或署名位置，保留署名声明 `Powered by OpenXPlay.`，并附上指向原项目的可见链接：<https://github.com/OpenXPlay/OpenXPlay-Relay>。详见 [NOTICE](./NOTICE)。

CarPlay 协议 crate `rust/carplay-iap2` 和 `rust/carplay-airplay`，以及 `rust/direlay-linux/src/network/` 中的少量文件，派生自 DiPlay 和 xcertplay。它们仍采用 [GNU General Public License v3.0](./LICENSE-GPL-3.0)，仅限第 3 版；GPLv3 第 13 条允许将它们与本 AGPLv3 项目组合。构建产物还包含 OpenXPlay-Stream（AGPLv3），因此中继会向所有通过网络使用它的人提供源代码，包括通过车机浏览器使用的人。

如果贵组织的政策不允许使用 AGPLv3 许可的软件，或你希望免于履行 AGPLv3 的开源义务，请联系：[support@openxplay.org](mailto:support@openxplay.org)。豁免范围为 OpenXPlay 拥有的代码；派生自 DiPlay 和 xcertplay 的部分仍受 GPLv3 约束。

CarPlay 和 iPhone 为 Apple Inc. 的商标；OpenXPlay 为独立项目，与 Apple 不存在隶属、认可或认证关系。本仓库及其构建产物不包含任何 Apple 配件（MFi）身份凭据；要正常使用，需由用户自行提供。
