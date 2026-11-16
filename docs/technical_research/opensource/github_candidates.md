# GitHub 候选项目核验（github_candidates.md）

> 核验方式：WebSearch 多源交叉 + README/文档研读 · 核验时间 2026-11-16 · Stars 等数据为调研时点快照

## 核验清单

### 1. Streamlink（架构借鉴）
- **仓库**：https://github.com/streamlink/streamlink
- **数据**：11.2k★ / 1.2k fork / 283 贡献者 / v8.x（2025-12 仍月更）/ BSD-2-Clause / Python
- **核心研究**：插件制直播流提取（twitch/douyu/huya/bilibili 等 100+ 插件）；每个插件=独立类（can_handle_url + 返回流地址族）
- **对 StreamFlow 的适配**：Python 不可直接依赖；**借鉴点**=插件式适配器组织 + 国内平台插件的存在证明协议可逆向稳定维护
- **结论**：架构借鉴

### 2. DTV / chen-zeong/DTV（**最重要可比对象**）
- **仓库**：https://github.com/chen-zeong/DTV
- **数据**：2,027★ / 92 fork / MIT / TypeScript+Rust / **Tauri 2.0（同栈）** / v3.0.3（2026-05）/ 最后 push 1 个月前 / 单维护者
- **平台覆盖**：斗鱼✅ 虎牙✅ B站✅ 抖音✅（**仅房间号进入，无搜索**）；**无快手**
- **核心研究**：斗鱼流参考 @wbt5/real-url；抖音弹幕参考 @saermart/DouyinLiveWebFetcher；播放器用 xgplayer（字节）；弹幕只读聊天（与 StreamFlow 同设计取舍）
- **对 StreamFlow 的适配**：同技术栈可直接读其 src-tauri 适配器源码做**协议实现对照**（特别是斗鱼流地址与虎牙签名算法）；**StreamFlow 功能面更广**（+快手、+抖音搜索/关注/热门、+历史/设置/更新）
- **结论**：架构借鉴 + 协议实现对照（MIT 允许代码级复用）

### 3. blivedm_rs（B 站弹幕对比）
- **仓库**：https://github.com/jiahaoxiang2000/blivedm_rs
- **数据**：MIT/Apache-2.0 双许可 / Rust 2024 / 0.5.5（2026-03）/ 4.5k SLoC / 活跃但单维护者（7★ 小众）
- **核心研究**：WebSocket 心跳 20s、WBI 签名、浏览器 cookie 自动检测、Scheduler+EventHandler 插件架构
- **对 StreamFlow 的适配**：StreamFlow 的 danmaku/bilibili.rs 已实现且工作正常（协议一致）；**借鉴点**=心跳与消息解析的健壮性处理、重连模式
- **结论**：架构借鉴（替换收益不足——现有实现已稳定）

### 4. tauri-plugin-libmpv（播放器替换候选）
- **仓库**：https://github.com/nini22P/tauri-plugin-mpv
- **数据**：MPL-2.0 / 0.3.2（2025-11-24）/ Windows 完整测试、Linux/macOS 未测试 / 需分发 libmpv-2.dll（+30MB 级）
- **核心研究**：libmpv 嵌入 Tauri 窗口（透明窗口+原生 surface）、hwdec=auto-safe、属性观察 API
- **对 StreamFlow 的适配**：**硬解+低占用优势明显，但**：① 原生 surface 会破坏 DOM 飘屏（需 OpenPlayer 式双窗口透明 overlay 方案，复杂度大增）② 分发动态库 ③ 全平台支持不均
- **结论**：暂缓处理（P2 观察项——仅当多开播放性能实际成为瓶颈时重启评估）

### 5. OpenPlayer（播放器架构参考）
- **仓库**：https://github.com/AreChen/OpenPlayer
- **数据**：MIT / Tauri 2 + React + libmpv / v1.6.8 活跃
- **核心研究**：video host + transparent overlay 双窗口模式——原生播放与 DOM UI 分层
- **结论**：架构借鉴（若未来上 mpv，分层方案照此）

### 6. DDTV（同类功能参照）
- **仓库**：https://github.com/CO2mega/DDTV
- **数据**：1.5k★ / C# .NET / GPL-2.0 / 活跃（29 天前有提交）
- **核心研究**：B 站单平台（录制向：自动录制+弹幕时间轴+转码）
- **结论**：仅功能参照（GPL-2.0 **不可代码复用**，技术栈不同）

### 7. WS 重连模式（标准模式，无需引依赖）
- **核验**：durable-ws-client（5★，模式参考）、tokio-retry（146★ MIT）、dev.to 生产实践（指数退避 1s→30s 封顶+成功重置）
- **结论**：**架构借鉴自主实现**——重连循环约 30 行，不值得引依赖

### 8. bilibili-API-collect（协议文档）
- **仓库**：https://github.com/SocialSisterYi/bilibili-API-collect
- **数据**：社区维护的 B 站非官方 API 集合文档
- **结论**：协议参考资料（已在其覆盖范围内）

## 未核验/受限项
- wbt5/real-url（JS 片段集合，非标准仓库结构）——未做代码级核验
- xgplayer（DTV 所用播放器）——未深入（Web 播放器与 hls.js 同类，无替换动机）
