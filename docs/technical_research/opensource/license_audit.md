# 许可证审查（license_audit.md）

> 原则：区分「可直接依赖 / 义务复用 / 仅架构借鉴 / 需人工审核 / 不建议」；无许可证仓库不视为允许复制

## 候选项目许可证

| 项目 | 许可证 | 分类 | 说明 |
|---|---|---|---|
| streamlink | BSD-2-Clause | 可直接依赖（Python，不适用）/ **仅架构借鉴** | 保留版权声明即可 |
| DTV (chen-zeong) | MIT | **可代码级复用** | 保留版权+许可声明；douyu/huya 协议实现可参考甚至移植 |
| blivedm_rs | MIT / Apache-2.0 双许可 | 可代码级复用 | MIT 路径需保留声明；Apache 路径需 NOTICE |
| tauri-plugin-libmpv | MPL-2.0 | 可直接依赖（库级链接，文件级 copyleft 不传染应用） | 修改该库源码需开源该文件 |
| OpenPlayer | MIT | 可代码级复用 | 双窗口 overlay 架构可参考实现 |
| DDTV | **GPL-2.0** | **仅功能参照，禁止代码复用** | 若 StreamFlow 分发（MSI），链接/复用其代码将触发 GPL 开源义务 |
| tokio-retry | MIT | 可直接依赖 | （最终未引入） |
| bilibili-API-collect | 见仓库（文档类） | 参考资料 | — |

## 现有依赖许可证（无风险确认）

| 依赖 | 许可证 | 风险 |
|---|---|---|
| tauri 2 / tauri-plugin-updater | MIT/Apache-2.0 | 无 |
| tokio / sqlx / reqwest / serde 全家桶 | MIT/Apache-2.0 | 无 |
| react 19 / hls.js / mpegts.js | MIT / Apache-2.0 | 无 |
| qrcode | MIT（default-features=false） | 无 |
| tokio-tungstenite | MIT/Apache-2.0 | 无 |

## 供应链风险检查
- 无来源不明的安装脚本；构建链为 cargo/pnpm 标准工具链
- updater 签名私钥本地保管（~/.tauri/streamflow-updater.key）✓ 未入库
- 建议（P2）：CI 加 `cargo deny`（许可证+漏洞审计自动化）

## 平台协议合规声明（非许可证但相关）
- 全部平台数据来自公开 Web 端协议重建，无官方 API 授权——DTV/StreamFlow 均声明「仅供学习」；StreamFlow README 已含同等声明 ✓
