# StreamFlow 中期技术审查与开源调研

> 调研时间：2026-11-16 · 基线：main @ 86d0099 · 审查方式：双 Explore 子代理（后端/前端）+ 人工核验 GitHub 候选
>
> 原则：以当前代码为事实依据 · 先问题后方案 · 最小修改 · 默认保留现有实现

## 文档导航

| 文档 | 内容 |
|---|---|
| [project_baseline.md](project_baseline.md) | 调研时技术栈、模块状态、依赖版本、Git 状态 |
| [current_code_audit.md](current_code_audit.md) | 各模块代码质量、完成度、已知问题（后端+前端审查整合） |
| [github_candidates.md](github_candidates.md) | 核验过的 GitHub 候选项目（stars/活跃/许可证/代码研究） |
| [candidate_comparison.md](candidate_comparison.md) | 候选项目 vs StreamFlow 现有实现对比 |
| [license_audit.md](license_audit.md) | 许可证与复用限制 |
| [dependency_compatibility.md](dependency_compatibility.md) | 依赖兼容性评估 |
| [optimization_opportunities.md](optimization_opportunities.md) | P0-P3 优化机会清单 |
| [reuse_decisions.md](reuse_decisions.md) | 每模块最终决策（保留/优化/复用/替换） |
| [integration_risks.md](integration_risks.md) | 集成风险 |
| [final_recommendations.md](final_recommendations.md) | 最终技术路线 |

## 一句话结论

架构方向正确（Tauri 2 + React + 平台适配器模式），**无重复造轮子**（播放器用 hls.js/mpegts.js、DB 用 sqlx、协议解析自主实现是合理的）；主要问题集中在**稳定性缺口**（弹幕无重连、UTF-8 切片 panic、签名临时文件竞态）和**死依赖清理**（sea-orm/tracing/thiserror 零使用）；所有修复均为局部优化级别，无需引入新的重型依赖。
