# 项目基线（project_baseline.md）

> 调研时点：2026-11-16 · Git：main @ 86d0099 · 工作区干净（无未提交修改）

## 技术栈（以实际配置为准）

| 层 | 技术 | 版本 | 证据 |
|---|---|---|---|
| 桌面框架 | Tauri | 2.0（tray-icon feature） | src-tauri/Cargo.toml |
| 后端语言 | Rust | edition 2021 | Cargo.toml |
| 异步运行时 | tokio | 1（rt-multi-thread/macros/sync/time/net/io-util） | Cargo.toml |
| 数据库 | SQLite | sqlx 0.7（sqlite/runtime-tokio-native-tls/macros/chrono） | Cargo.toml |
| ORM（未使用） | sea-orm | 1.0（**代码零引用**） | Cargo.toml |
| HTTP | reqwest | 0.12（json/native-tls/gzip/**rustls-tls 双后端**） | Cargo.toml |
| WebSocket | tokio-tungstenite | 0.24（native-tls） | Cargo.toml |
| 日志 | log 0.4 + env_logger 0.11；**tracing 0.1 + tracing-subscriber（零使用）** | 双套并存 | Cargo.toml |
| 错误 | anyhow 1.0；**thiserror 1.0（零使用）** | | Cargo.toml |
| 前端框架 | React | 19.3.1 | frontend/package.json |
| 播放器 | hls.js 1.7.4 + mpegts.js 1.8.2（浏览器内嵌，无原生内核） | | package.json |
| 状态管理 | React Context 手写（无状态库） | | frontend/src/context.tsx |
| 打包 | MSI（WiX light），含 updater plugin 2 | | tauri.conf.json |

## 代码规模

| 部分 | 行数 | 文件数 |
|---|---|---|
| 后端 Rust（src-tauri/src） | 8,964 | 33 |
| 前端 TS/TSX（frontend/src） | 3,602 | ~30 |
| 平台适配器 | 3,603 | 7（5 平台+mock+mod） |
| 弹幕模块 | 2,103 | 7 |

## 模块结构与状态

| 模块 | 文件 | 状态 | 依据 |
|---|---|---|---|
| 平台适配器 | platforms/{bilibili,douyin,douyu,huya,kuaishou,mock}.rs | 已完成且稳定（游客播放/搜索/热门），抖音部分功能依赖登录态 | 实测搜索 12 条→详情→4 档画质流；冒烟 14/14 |
| 弹幕 | danmaku/{5 平台,proto}.rs | 已完成但存在问题（无自动重连） | 代码审查 D2（danmaku/mod.rs:118-148） |
| 逆向/签名 | reverse/{webctx,abogus,bogus,huya_sig,ks_ws}.rs | 基本完成，维护成本高（平台协议变更即失效） | webctx.rs 实测可用 |
| 数据库 | database/{history,rooms,settings}.rs + db.rs | 已完成且稳定 | sqlx 迁移+3 个仓储 |
| 命令层 | commands.rs | 已完成且稳定 | 全部 tauri command 汇聚 |
| 播放 | 前端 hls.js/mpegts.js（无后端播放模块） | 已完成且稳定 | WatchPage.tsx:62-89 |
| 飘屏 | 前端 DanmakuOverlay.tsx | 已完成（全屏已支持） | 用户实测通过 |
| 认证 | auth.rs | 已完成且稳定 | 5 平台登录态管理 |
| 托盘 | tray.rs | 已完成 | Cargo feature tray-icon |
| 队列 | queue.rs | **疑似死模块**（审查确认引用情况） | 代码审查 |

## 功能完成度清单（15 项）

| 功能 | 状态 | 证据 |
|---|---|---|
| 1 多平台统一接入 | ✅ 稳定 | 5 平台适配器+冒烟 14/14 |
| 2 跨平台搜索 | ✅ 稳定（抖音依赖登录态） | webctx 实测 12 条 |
| 3 主播/直播间信息 | ✅ 稳定 | room_detail 实测 |
| 4 直播状态检测 | ✅ 稳定 | 失效监测（接口级） |
| 5 直播流解析 | ✅ 稳定 | reflow/HLS/FLV 多路径 |
| 6 视频播放 | ✅ 稳定（软解，多窗口=单页切换） | hls.js/mpegts.js |
| 7 收藏与关注管理 | ✅ 稳定（抖音关注 11-13 打通） | 38 条收藏实测 |
| 8 开播提醒 | ✅ 完成 | 首页"关注在播"条 |
| 9 弹幕及实时消息 | ⚠️ 已完成但无自动重连 | 审查 D2 |
| 10 本地数据管理 | ✅ 稳定 | SQLite 3 仓储 |
| 11 观看历史 | ✅ 稳定（含保留期限） | 42 条实测 |
| 12 平台管理 | ✅ 稳定 | 6.1 已交付 |
| 13 播放器管理 | ✅ 稳定 | 音量记忆/静音/画质 |
| 14 系统设置 | ✅ 稳定 | settings 仓储 |
| 15 日志/异常 | ⚠️ 基本完成（env_logger，无结构化/无上报） | 审查 E2 |
| 16 自动更新 | ✅ 稳定 | v0.0.2 实测签名验证 |
