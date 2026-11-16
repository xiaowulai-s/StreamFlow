# 依赖兼容性（dependency_compatibility.md）

> 原则：不为小功能升级底层依赖；不改异步运行时；不加重叠播放器内核；Cargo.lock 变更需兼容性验证

## 当前环境
- Rust edition 2021（stable 工具链）/ tokio 1 / reqwest 0.12 / sqlx 0.7 / serde 1 / Tauri 2 / Node（前端构建）/ React 19 / SQLite（sqlx 内嵌）

## 候选引入评估

| 候选 | 版本 | 与现栈兼容性 | 依赖增量 | 编译时间影响 | 结论 |
|---|---|---|---|---|---|
| tauri-plugin-libmpv | 0.3.2 | ✅ Tauri 2 原生插件 | +glutin/winit/libmpv2-sys + 运行时动态库 30MB+ | +2-3min | 暂缓（触发条件见 candidate_comparison） |
| tokio-retry | 0.3.2 | ✅ 仅 tokio::time feature | +pin-project-lite 级别 | 可忽略 | 未引入（手写 30 行重连） |
| react-window / virtua | 最新 | ✅ React 19 兼容（需验证 virtua 对 React 19 支持版本） | 小 | 可忽略 | 暂缓（P3） |
| tauri-plugin-log | 2.x | ✅ Tauri 2 | 小 | 小 | 暂缓（当前 env_logger 够用） |
| cargo-deny（CI） | 最新 | ✅ 工具链级 | 无运行时影响 | — | 建议 P2 引入（许可证/漏洞审计） |

## 需要清理的现存依赖（删而不是加）

| 依赖 | 操作 | 影响 |
|---|---|---|
| sea-orm 1.0 | 从 Cargo.toml 删除 | 编译时间 -30~60s、产物体积下降；sqlx 为唯一 DB 层 |
| tracing + tracing-subscriber | 删除 | 与 log/env_logger 二选一已完成 |
| thiserror | 删除 | 错误统一走 anyhow |
| reqwest native-tls feature | 评估删除（保 rustls） | 需回归验证 5 平台请求（快手已用 rustls ✓；其余平台 TLS 握手无特殊指纹需求） |

## 不兼容/高风险项
- **无阻塞性不兼容**。全部候选均为增量式引入，不动 tokio 运行时与 DB 层。
- libmpv 若引入：窗口需 `transparent: true`（影响现有窗口合成与飘屏 DOM 渲染策略）——这是它被归为「暂缓」的核心原因之一。
