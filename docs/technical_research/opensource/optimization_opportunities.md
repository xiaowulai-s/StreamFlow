# 优化机会清单（optimization_opportunities.md）

> 每条含：问题 / 位置 / 原因 / 方案 / 收益 / 影响范围 / 风险 / 验证方式 / 优先级

## P1（应当优化）

### OPP-1 弹幕自动重连（核心稳定性）
- **问题**：弹幕任务任一次 WS 错误即永久终止，仅 emit error 状态
- **位置**：`src-tauri/src/danmaku/mod.rs:118-148`（run 包裹层）；5 平台弹幕循环均在 `danmaku/{platform}.rs` 内层
- **原因**：任务模型一次性设计（连接→循环→返回），无恢复语义
- **方案**：mod.rs 的 spawn 包裹层加重连循环：`loop { run(...).await; backoff = min(backoff*2, 30s); sleep; backoff 重置条件=上次存活>60s }` + emit `reconnecting` 状态；平台侧无需改动
- **收益**：网络抖动自愈，弹幕体验质变（一处修复全平台）
- **影响**：仅 danmaku/mod.rs；前端 status 事件多一种（需前端适配显示）
- **风险**：低（循环包裹不改内层逻辑）；房间下播时需终止重连（检测 status）
- **验证**：断网 5s 恢复 → 弹幕自动恢复；下播房间不无限重连
- **优先级**：P1

### OPP-2 UTF-8 字节切片 panic
- **问题**：3 处 `&s[..n]` 按字节切中文串，多字节边界 panic
- **位置**：`platforms/douyu.rs:241-246, 570-573`；`reverse/webctx.rs:253-262`
- **方案**：统一 `chars().take(n).collect()`（或 truncate_utf8 辅助函数）
- **收益**：消除解析路径上的 panic 类崩溃
- **影响**：3 处小改
- **风险**：无
- **验证**：构造含 emoji 的响应单元测试
- **优先级**：P1

### OPP-3 斗鱼签名文件竞态 + 同步阻塞
- **问题**：`sign_via_js` 固定临时文件路径并发互踩；同步 fs/process 阻塞 async worker
- **位置**：`platforms/douyu.rs:520-565, 598-619`
- **方案**：签名 HTML `include_str!` 内嵌；临时文件用 uuid 名（或纯 webview eval 注入，与 webctx 同路）；IO 包 `spawn_blocking`
- **收益**：并发斗鱼请求正确性 + worker 不阻塞
- **影响**：douyu.rs 单文件
- **风险**：低
- **验证**：并发 3 路 room_detail 无签名错乱
- **优先级**：P1

### OPP-4 死依赖清理
- **问题**：sea-orm（0 引用）、tracing（0 匹配）、thiserror（0 匹配）死依赖；reqwest 双 TLS
- **位置**：Cargo.toml
- **方案**：删三依赖；native-tls→rustls 单后端（需 5 平台回归）
- **收益**：编译 -30~60s、体积下降、供应链面缩小
- **影响**：Cargo.toml/Cargo.lock；无代码改动（除 reqwest feature）
- **风险**：低（rustls 回归验证兜底）
- **验证**：5 平台搜索/详情/流请求全通 + 冒烟 14/14
- **优先级**：P1（清理）/P2（TLS 统一）

## P2（可以改进）

### OPP-5 tauri listen unlisten 泄漏（前端）
- **问题**：`await listenEvent` 注册晚于 cleanup——快速换房累积泄漏监听器
- **位置**：`frontend/src/components/DanmakuPanel.tsx:50-58`；同模式在 WatchPage 弹幕/状态订阅
- **方案**：unlisten 完成后校验 disposed 再补 unlisten（`if (disposed) unlisten()`）
- **收益**：消除长会话内存增长
- **风险**：无。**优先级**：P2

### OPP-6 searchScope 变化不重搜（前端功能缺陷）
- **问题**：`lastSearch.current !== keyword` 守卫阻断范围切换后的重搜
- **位置**：`frontend/src/pages/HomePage.tsx:70-79`
- **方案**：守卫改为 `keyword+scope` 组合键
- **风险**：无。**优先级**：P2

### OPP-7 reverse/ 死代码清理
- **问题**：douyin_abogus/bogus/dy_search_probe/kuaishou_ws 已被 webctx 路线替代；queue.rs 疑似死模块
- **方案**：grep 确认零引用后删除（git 历史可找回）
- **收益**：维护面缩小。**风险**：低（需确认 huya_signature 仍在用保留）
- **优先级**：P2

### OPP-8 webctx 单槽竞态
- **问题**：结果槽 Mutex<Option<String>> 并发覆盖
- **位置**：reverse/webctx.rs:44-58
- **方案**：请求带自增 id，回传携带 id 匹配（或调用方全局串行化——Mutex<bool> 占用标志）
- **收益**：为未来并发 fetch 铺路
- **风险**：低。**优先级**：P2

### OPP-9 hls.js 网络错误自动恢复（前端）
- **位置**：WatchPage.tsx hls ERROR 分支
- **方案**：`NETWORK_ERROR` 时 `hls.startLoad()` 有限次重试
- **优先级**：P2

### OPP-10 cargo-deny CI 审计
- **方案**：CI 加 cargo-deny（许可证+RUSTSEC 漏洞）
- **优先级**：P2

## P3（暂缓）
- **OPP-11 虚拟化列表**：千级卡片再引入（当前百级无问题）
- **OPP-12 libmpv 播放器**：多开性能成为实际瓶颈再评估（飘屏需迁移 OpenPlayer 式分层）
- **OPP-13 结构化日志/崩溃上报**：用户量级需要时再做
- **OPP-14 CI/CD 完整流水线**：当前手动构建发布流程可用
- **OPP-15 集成测试**：解析类单测已有 11 个；补平台请求的契约测试（mock 化）P3
