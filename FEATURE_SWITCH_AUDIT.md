<!-- SPDX-FileCopyrightText: 2026 OpenXPlay.org -->
<!-- SPDX-License-Identifier: AGPL-3.0-only -->

# 功能开关盘点

日期：2026-10-25。

范围：工作区源码与标准发布配置，并包含本次新增的实验开关实现。初始盘点的基础版本 Relay 为 `d0961d3`，Stream 为 `eff0e90`，DiPlay 为 `8936b98d`；当时后两者与 Relay 的锁文件一致。初始盘点未解包核验已发布的 APK 或 Debian 包，也未读取设备上的实际配置。下述默认值指首次安装或相应配置项缺失时的行为，已有用户设置和迁移可能改变它。

## 结论

Android 原有四个 Carlink 实验选项；本次新增三个连接实验选项，共七个，均默认关闭并受实验总开关控制。未发现用产品 flavor、BuildConfig 业务功能标志或 Relay 自身的 Cargo feature 在标准发布版本中关闭这些功能。开发者设置和管理网页的显示开关也需要与实际功能开关区分。

本次已把三项连接调整接入独立开关，Relay 单元测试与调试 APK 构建、Stream Carlink 测试、DiPlay 完整测试/lint/示例构建均通过，并已核对本地调试 APK 的中英文设置资源。双无线实机验证尚未完成：

| 开关 | 链路分组 | 影响范围 |
| --- | --- | --- |
| `isolateCarlinkNetwork` | 中继 ↔ 车机 | Carlink；双输出时也影响浏览器的网络选择。使用连接级 socket 绑定，不改变整个进程的网络绑定 |
| `waitForCarlink` | 中继 ↔ 车机 | 无线 CarPlay 使用 Carlink 时，最多等待一分钟；双输出时也延后浏览器画面启动 |
| `recoverWirelessNetwork` | 中继 ↔ iPhone | 浏览器和 Carlink 的无线 CarPlay；校验网络、检测变化并为 TCP 后的会话建立增加超时 |

上述开关及原有选项均支持本机保存、编辑状态恢复与配置导入导出。改变设置不立即断开正在运行的连接，按界面说明在下一次连接或启动时生效。

## Android：原有四个默认关闭的实验选项

入口：设置 → 实验功能 → 使用实验功能。`experimental.active` 和所有子选项均默认 `false`；总开关关闭时，运行时会获得全关闭的 `ExperimentalOptions()`，已保存的子选项不会生效。更改在下一次 Carlink 连接创建时应用。

| 配置项 | 实际行为 | 附加条件 |
| --- | --- | --- |
| `rememberSubscription` | 将车机成功采用的旧式 BLE 订阅行为持久化；后续连接可避免再次等待 CCCD 超时 | 默认已有请求类型记忆和本次运行内的订阅回退；此项增加的是跨运行的订阅方式记忆 |
| `keepScanning` | 将每次 8 秒的 BLE 扫描改为持续等待，并每 10 分钟重新启动扫描 | 作用于寻找 Carlink 车机，不能替代 iPhone 侧会话恢复 |
| `recordAdvertisements` | 记录扫描期间的原始 BLE 广播信息 | 同时要求 `carlinkTrace=true`，否则 `effective()` 强制关闭；界面还提示保持亮屏 |
| `sendBluetoothAddress` | 使用用户填写的蓝牙地址替换自动生成的 Carlink 握手地址 | 地址须通过校验；不会修改 Android 系统蓝牙硬件地址 |

证据：`android/app/src/main/java/org/openxplay/direlay/settings/ExperimentalOptions.kt:6`；`relay/RelayRuntime.kt:1079`；`clink/AndroidClinkLink.kt:56`；`clink/BleScanPolicy.kt:6`；`clink/BlePeerPolicy.kt:27`；`ui/ExperimentalScreen.kt:15`（后五个路径均在同一 Java 包目录下）。

四个选项主要影响中继到车机的 BLE 发现与引导阶段。输出已正常而 iPhone 在 Wi-Fi 上无法建立 CarPlay 会话时，不能据此认定打开这些开关即可修复。

## Android：其他默认关闭或需要显式配置的功能

| 功能 | 默认值或条件 | 入口或限制 |
| --- | --- | --- |
| Carlink 输出 | `carlinkOutput=false`，初始为浏览器输出 | 设置向导或输出设置可选择；不表示未编译 |
| 浏览器与 Carlink 同时输出 | `bothOutputs=false` | 开发者 → 连接与输出；这是双输出，与双无线链路不同 |
| 车机窗口变化时重连 | `carlinkReconnectOnResize=false` | 输出设置；运行时已有重连策略 |
| 车辆信息采集 | `vehicleInformation=false` | Carlink 输出设置；已接入 SENSOR 与采集逻辑 |
| 本机预览、本机播放音频 | `localPreview=false`、`playCarPlayAudio=false` | 开发者 → 连接与输出；还受开发者总开关控制 |
| Carlink 完整调试日志 | `carlinkTrace=false` | 开发者 → Carlink；还受开发者总开关控制 |
| DLNA、Miracast | `dlnaOutput=false`、`miracastOutput=false` | 输出设置；DLNA 需选择接收器，Miracast 还依赖系统投屏能力与权限 |
| Tesla 浏览器地址方案 | `teslaBrowser=OFF` | 车型/车载浏览器设置；依赖无线网络、地址方案及相应配置 |
| 视频统计显示 | `videoStats=OFF` | 画面设置；统计功能已有实现 |
| 随系统启动 | `autoStartOnBoot=false` | 启动设置；与默认开启的准备就绪后自动连接不同 |
| 插件框架 | `plugins=false`，`pluginsEnabled=[]` | 总开关、插件选择与实际插件资源均需满足；见下面的发布资源问题 |
| 可信 HTTPS 域名与证书 | 未配置时不运行该服务 | 需配置提供商、域名与凭据；不能只看配置文件内 `enabled` 的默认解析值 |

默认值与加载逻辑：`settings/RelaySettings.kt:49`、`:240`、`:415`、`:440`；开发者规则：`settings/DeveloperOverrides.kt:9`。DLNA/Miracast 的执行入口为 `relay/RelayRuntime.kt` 的 `configureDlna()` 和 `configureMiracast()`。随系统启动的底层默认值为 DiPlay 的 `common/src/main/java/com/shilapi/xcertplay/AirPlayPersistence.kt:499`。

## 容易误判的开关

- Android 开发者总开关 `developer.active` 默认是 `true`，实验总开关 `experimental.active` 默认是 `false`，两者独立。
- `carlinkHevcEnabled=false` 关闭的是开发者的 HEVC 手动覆盖；实际回到 `STREAM_ONLY`，并非关闭全部 H.265 支持。分辨率、麦克风来源、网页端口等 `...Enabled` 也属于覆盖开关。
- `carlinkAutoReconnect=true`、`viewAreas=true`、Android 的 `auto_connect=true`，这些现有行为已经默认开启。
- Android 会一次性迁移为 `carBluetoothAudio=true`，Linux 也默认为 `true`：音频留在 iPhone 到车机的蓝牙链路。这与中继本机播放音频默认关闭是不同设置。
- 管理网页的开发者选项默认隐藏技术设置，可以在页面里开启，也支持 `?dev=1`。它控制字段显示，不会自动打开后端的实验功能。Android 管理 API 的设置 schema 当前也没有暴露这七个实验选项，需通过本机界面或配置导入修改。

证据：`settings/DeveloperOverrides.kt:27`；`settings/RelaySettings.kt:217`、`:273`、`:441`；`settings/CarBluetoothAudioDefaults.kt:10`；`admin-web/src/logic.ts:87`、`:107`；`android/app/src/main/java/org/openxplay/direlay/admin/AdminSettingsMapping.kt:20`。

## 插件资源的发布缺口

框架有运行时开关，但标准发布工作流没有检出或构建 `OpenXPlay-Web-Plugins`：

- Android 从可选的 `../../OpenXPlay-Web-Plugins/dist/install` 或 `-Pplugins.dir` 复制插件。目录不存在时，`PluginAssets.copy(null, ...)` 只产生空输出，不会阻止构建。当前工作区没有该构建目录。
- Android 的标准发布任务检出 Relay、Stream 和 DiPlay，提供管理页面和浏览器接收端资源，但没有提供插件构建产物，也没有检查 `assets/plugins`。
- Linux 的标准打包步骤复制两个可执行文件、浏览器页面与管理页面等文件，没有复制插件资源目录。
- Android 的实际插件配置要求总开关开启，且至少一个选中的插件确实存在；只有 `plugins=true` 仍可能返回空配置。

因此，按当前标准发布流程，插件框架开启后预计仍没有内置插件可选。自定义构建或手工部署插件资源可以改变这个结果；尚未解包核验具体已发布二进制文件。

证据：`android/app/build.gradle.kts:130`；`android/buildSrc/src/main/kotlin/org/openxplay/plugins/CopyPluginAssets.kt:28`；`android/app/src/main/java/org/openxplay/direlay/plugins/RelayPlugins.kt:59`；`.github/workflows/release-android.yml:134`、`:198`；`.github/workflows/release-linux.yml:144`。

## Linux 范围

Linux 默认关闭的设置包括插件、接收端配对码要求、窗口尺寸变化后重连、Carlink 调试日志和视频统计。Carlink 输出初始未选中，初始输出为 `web`；自动重连、自动启动和视区支持已经默认开启，HEVC 为 `auto`。

Android 的七个实验选项未出现在 Linux 的同名可控设置中；这不意味着 Linux 一定没有类似的底层行为。Linux 已有的 Carlink 网络跟随切换和控制器重启也不是上述实验总开关控制的。

证据：`rust/direlay-linux/src/settings.rs:67`；`rust/direlay-linux/src/relay_admin.rs:1610`；`rust/direlay-linux/src/carlink_follow.rs`。
