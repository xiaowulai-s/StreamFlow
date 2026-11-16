# 复用决策（reuse_decisions.md）

> 决策类型：保留现有实现 / 局部优化 / 引入第三方依赖 / 局部代码复用 / 架构借鉴 / 模块替换 / 暂缓处理 / 淘汰

## 决策总表

| 模块 | 当前实现 | 调研发现 | 最终建议 | 优先级 |
|---|---|---|---|---|
| 平台适配（5 平台） | 自研 Rust 适配器，trait 统一 | DTV 同栈实现可对照（MIT）；streamlink 插件制架构 | **保留** + 协议变更时对照 DTV/real-url | — |
| 平台适配重复代码 | 跨适配器 150-200 行样板 | — | **局部优化**：抽 reqwest get_json 公共 helper | P2 |
| 抖音逆向 | webctx 页面环境架构（已替代 a_bogus 移植路线） | DTV 无抖音搜索（我们的实现更完整） | **保留** | — |
| 斗鱼签名 | 临时文件 JS 签名 | DTV/real-url 的实现方式 | **局部优化**：内嵌+uuid 文件名+spawn_blocking（OPP-3） | P1 |
| 弹幕（5 平台） | 自研 WS 协议实现 | blivedm_rs（B 站）健壮性可借鉴 | **局部优化**：统一加重连循环（OPP-1）+ 心跳边界补强 | P1 |
| 播放器 | hls.js/mpegts.js 浏览器内嵌 | libmpv 硬解优势 vs 飘屏 DOM 依赖 | **保留** + P2 观察（OPP-12 触发条件：多开瓶颈） | — |
| 播放解析 | hls.js/mpegts.js 成熟库 | 无重复实现（FLV/HLS 未手写） | **保留现有实现** | — |
| 前端列表 | 直接渲染（百级卡片） | react-window/virtua | **暂缓**（千级再引入） | P3 |
| 数据库 | sqlx 单层 | sea-orm 死依赖 | **局部优化**：删 sea-orm（OPP-4） | P1 |
| 日志 | log+env_logger | tracing 死依赖；tauri-plugin-log | **局部优化**：删 tracing；插件暂缓 | P1/P3 |
| 网络重试/重连 | 手写 | 标准退避模式（tokio-retry 未引入） | **架构借鉴自主实现**（OPP-1 重连） | P1 |
| 自动更新 | tauri-plugin-updater（实测） | — | **保留现有实现** | — |
| 错误处理 | anyhow 统一 | thiserror 死依赖 | **局部优化**：删 thiserror | P1 |
| 逆向死代码 | reverse/ 4 文件+queue.rs | 已被 webctx 替代 | **淘汰**（OPP-7，grep 确认后删） | P2 |
| 录制/转码 | 无 | DDTV 功能参照（GPL 禁复用） | **暂缓**（不在产品范围） | P3 |

## 关键决策理由

1. **播放器不换 mpv**：飘屏（产品差异化功能）依赖 DOM 叠加；libmpv 原生 surface 会推翻该架构。性能证据未出现前不动。
2. **B 站弹幕不引 blivedm_rs**：协议已实现且稳定；引入后集成层成本 > 补强现有代码。
3. **重连不引依赖**：30 行标准模式 vs 新依赖+适配层。
4. **DTV 只做对照**：功能面小于 StreamFlow，无替换动机；其斗鱼/虎牙协议实现（MIT）是协议变更时最可靠的对照源。
