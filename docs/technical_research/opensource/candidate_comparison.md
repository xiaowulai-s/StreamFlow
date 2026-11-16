# 候选对比（candidate_comparison.md）

> 逐项回答：解决什么问题 / StreamFlow 现状 / 对方优势 / 不足 / 兼容性 / 接口改动 / 模块影响 / 依赖增量 / 维护成本 / 是否值得替换

## 1. 平台适配器 vs DTV（Tauri 2 同栈）

| 维度 | StreamFlow 现状 | DTV |
|---|---|---|
| 平台覆盖 | 虎牙/斗鱼/B站/**抖音/快手** 全功能 | 斗鱼/虎牙/B站/抖音（无快手；抖音仅房间号） |
| 抖音搜索/关注/热门 | ✅ 页面环境架构（webctx） | ❌ 无搜索 |
| 适配器语言 | Rust（reqwest+自主签名） | Rust（参考 JS 社区方案移植） |
| 代码组织 | 5 文件各 500-800 行，trait 统一（adapter.rs PlatformAdapter） | 类似规模 |

**对比结论**：StreamFlow 功能面显著更广（快手 + 抖音全功能 + 失效监测 + 自更新）；DTV 优势在社区验证（2k 用户）与**斗鱼/虎牙协议实现的另一份参照**。
**决策**：**保留现有实现**；DTV 的 douyu/huya 适配源码作为协议变更时的对照参考（MIT 允许代码级复用）。

## 2. B 站弹幕 vs blivedm_rs

| 维度 | StreamFlow danmaku/bilibili.rs | blivedm_rs |
|---|---|---|
| 协议 | 已实现（认证/心跳/解析）同协议 | 同协议，更成熟的边界处理 |
| 集成 | 已与 RoomInfo/事件流集成 | 独立库，需适配层 |
| 重连 | **无自动重连（B2）** | 有错误处理但也不完善（issue #11 读消息错误断连） |

**决策**：**局部优化**——参考其心跳/解析健壮性补强自己的实现 + 统一加重连循环；不引入依赖（引入后仍需自己的重连与集成层，净收益为负）。

## 3. 播放器 vs libmpv（tauri-plugin-libmpv + OpenPlayer 模式）

| 维度 | StreamFlow（hls.js/mpegts.js） | libmpv 方案 |
|---|---|---|
| 解码 | 软解（WebView） | 硬解（hwdec=auto-safe） |
| 资源 | 多开内存/CPU 偏高 | 显著更低 |
| 飘屏 overlay | **DOM 直接叠加（现有方案核心优势）** | 原生 surface 遮挡 DOM——需双窗口透明 overlay（OpenPlayer 模式），复杂度大增 |
| 分发 | 无原生依赖（MSI 9.7MB） | +libmpv 动态库（~30MB+） |
| 平台支持 | WebView2（Windows 已验证） | Windows 已验证/Linux/macOS 未测试 |
| 画质切换 | 原生支持（多码率 URL） | 支持 |

**决策**：**保留现有实现 + P2 观察**。触发重评条件：多窗口/多开播放性能实际成为用户可感知瓶颈时（当前单窗口播放无问题）。若届时切换，飘屏需迁移到 OpenPlayer 式透明 overlay 双窗口架构。

## 4. 网络层 vs tokio-retry / durable-ws-client

| 维度 | StreamFlow 现状 | 成熟方案 |
|---|---|---|
| 请求重试 | 手写（快手重试已加） | tokio-retry（策略迭代器+抖动） |
| WS 重连 | **无（B2）** | 标准退避循环（1s→30s 封顶+成功重置） |

**决策**：**架构借鉴自主实现**——重连循环 ~30 行不值得引依赖；请求重试已手写，保持。

## 5. 前端列表 vs react-window/virtua

| 维度 | StreamFlow 现状 | 虚拟化 |
|---|---|---|
| 规模 | 搜索结果百级卡片 | 千级以上才显著 |
| 复杂度 | 无 | 引入依赖+滚动定位/卡片高度一致性要求 |

**决策**：**暂缓处理（P3）**——当前规模无性能问题证据；规模上千再引入。

## 6. 日志 vs tauri-plugin-log / tracing

**决策**：**局部优化**——删除死依赖 tracing（零使用）；保留 log+env_logger；如需文件日志/级别过滤后续可评估 tauri-plugin-log（暂无需求证据）。

## 7. 录制/转码 vs DDTV 能力

**决策**：**暂缓（P3）**——录制不在当前产品范围；如未来做，GPL-2.0 的 DDTV 只能功能参照，实现看 streamlink 式 pipe 或 ffmpeg 侧车。
