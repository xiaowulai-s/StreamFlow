# 现有代码审查（current_code_audit.md）

> 审查方式：双 Explore 子代理（后端 8,964 行 / 前端 3,602 行）· 每条结论附文件:行号证据

## P1 发现（应当处理）

### B1. UTF-8 字节切片 panic（3 处）
- `platforms/douyu.rs:241-246, 570-573`：`&text[..100]`、`&rawdata[..120]` 按字节切 UTF-8 字符串
- `reverse/webctx.rs:253-262`：诊断日志 `&s[..60]`
- **风险**：中文响应落在多字节字符中间时 `char boundary` panic → 命令整体失败（非正常错误路径，进程内不可恢复）
- **修复**：改用 `s.chars().take(n).collect::<String>()`（webctx 已有先例）

### B2. 弹幕连接无自动重连（核心稳定性缺口）
- `danmaku/mod.rs:118-148`：任务返回 `Err` → 仅 emit `error` 状态，任务终止，**不再恢复**
- **风险**：直播中网络抖动一次，弹幕永久静默，用户必须手动重进直播间
- **对照**：所有 5 平台弹幕任务共享此结构（一处修复全平台受益）
- **修复方向**：任务内重连循环 + 指数退避（1s→30s 封顶），成功后重置；参考模式已核验（见 github_candidates.md "WS 重连模式"）

### B3. 斗鱼签名临时文件竞态 + 同步阻塞
- `platforms/douyu.rs:520-565, 598-619`：`sign_via_js` 每次写 `%TEMP%/sf_dy_sign.html` 固定路径 → **并发请求互踩**；`std::fs` 同步 IO + `std::process::Command` 在 async 上下文阻塞 worker
- **修复**：签名 HTML 改 include_str! 内嵌 + 临时文件用 `uuid` 文件名；文件 IO 移入 `tokio::task::spawn_blocking` 或改为 webview eval 注入（与 webctx 同路）

### B4. 死依赖（编译时间与体积浪费）
| 依赖 | 声明位置 | 实际使用 | 结论 |
|---|---|---|---|
| sea-orm 1.0 | Cargo.toml | **0 处引用**（grep 全库） | 删除（连带减编译 30-60s 与数 MB 体积） |
| tracing + tracing-subscriber | Cargo.toml | **0 处匹配** | 删除 |
| thiserror 1.0 | Cargo.toml | **0 处匹配** | 删除 |
| reqwest 双 TLS 后端（native-tls + rustls-tls） | Cargo.toml | rustls 用于快手流请求 | 统一为 rustls 单后端（native-tls 可移除，需验证其他平台请求兼容性） |

## P2 发现（可以改进）

### 后端
- **webctx 单槽竞态**（reverse/webctx.rs:44-58）：`Mutex<Option<String>>` 结果槽 + Condvar——并发两个页面 fetch 会互相覆盖（当前调用方串行，暂无实害；**加调用序号匹配可根治**）
- **死接口脱节**：`danmaku_url()`（douyin.rs 返回 None）、`capabilities()` 的 DanmakuSupport 声明——弹幕实际走 danmaku/ 模块独立实现，trait 接口与实现脱节
- **跨适配器重复**：5 平台间重复的 HTTP GET+header+解析样板约 150-200 行（reqwest get_json 模式）——可抽公共 helper（局部优化，不动架构）
- **queue.rs / tray.rs**：queue.rs 疑似死模块（无调用方）；tray.rs 使用的 `Windows` feature 与功能清单核对
- **reverse/ 目录**：douyin_abogus.rs、douyin_bogus.rs、huya_signature.rs、kuaishou_ws.rs、dy_search_probe.rs——**5 个文件中 4 个已被 webctx 页面环境路线替代**（仅 huya_signature 仍在用）——死代码清理候选

### 前端
- **D1 tauri listen unlisten 泄漏**（DanmakuPanel.tsx:50-58 / WatchPage.tsx:69-89 模式）：`await listenEvent(...)` 注册晚于 cleanup 执行——快速切换房间时旧监听器泄漏累积（每条泄漏持续转发事件）——修复：`disposed` 标志在 unlisten 完成后二次校验
- **B1 searchScope 变化不重搜**（HomePage.tsx:70-79）：`lastSearch.current !== keyword` 守卫导致切换范围后 `doSearch` 被跳过——功能缺陷（上次引入）
- **B5 流格式判断**（WatchPage.tsx:139-153）：URL 后缀嗅探（.m3u8/.flv）——抖音 CDN 带查询参数时 `.flv?` 判断脆弱；**B6**：hls.js ERROR 分支未覆盖 ` NETWORK_ERROR` 自动恢复
- **A1/C1**：巨型 Context 全量订阅（任意 setState 全树重渲）+ 搜索结果无虚拟化（百级卡片，当前规模可接受）

### 前端亮点（做得好的）
- 飘屏 DanmakuOverlay：transform 动画 + 空闲行分配 + 密度上限——实现正确（实测通过）
- 契约对齐：`invoke camelCase` ↔ 后端 `serde(rename_all)` 全部一致；事件名 danmaku-message/status 两端一致
- 弹幕面板 dispose 标志 + unlisten（除 D1 时序洞外正确）；hls/mpegts destroy 分支齐全

## 稳定且不需要修改的模块
- database/（sqlx 迁移+仓储设计清晰）
- adapter.rs trait 设计（PlatformAdapter 抽象合理，新增平台成本低）
- auth.rs（登录态管理）
- 播放链路（get_stream_urls → hls.js/mpegts.js）
- updater（v0.0.2 实测）

## 技术债总结
1. **逆向签名层维护成本**（reverse/）：平台协议变更即失效——已用 webctx 页面环境路线大幅降低（无需 a_bogus）；剩余 huya_signature 一处
2. **双 ORM/双日志/双 TLS**：历史演进残留，清理成本低收益明确
3. **测试覆盖**：仅 11 个单测（解析类），无集成测试/CI
