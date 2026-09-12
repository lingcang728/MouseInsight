# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0-beta.1] - 2026-09-12

基于 `MouseInsight.md` 全量审计的大规模修复版本：共复核 100+ 项疑似问题，剔除 2 项误报，其余确认项全部修复或明确标注为实机验证项。

### Added

- 结构化日志：`tauri-plugin-log`，日志写入 `<配置目录>/logs/`，最多保留 3 份、单份 1MB；托盘新增「打开日志目录…」。日志不记录任何按键内容。
- Panic 钩子：崩溃时写 `logs/crash-*.log` 并尽力释放所有注入按键。
- 引擎看门狗：worker 线程 panic / 卡死时自动释放全部注入按键并推送 `engine-fatal`；物理按键状态看门狗检测「物理已松开但状态机仍认为按住」的失步并自动纠正。
- 钩子心跳：低层钩子静默失效时自动重建（空闲 60s 触发）；`hook-status-changed` 事件实时推送前端，前端提供「重试监听」按钮。
- 前端告警系统：sticky 告警、超时告警、动作按钮（撤销删除 / 打开辅助功能设置 / 重试监听 / 重新检测 X-Mouse）。
- 映射删除 5 秒撤销。
- 新增命令：`retry_hook`、`open_accessibility_settings`（macOS）、`press_record_key`。
- macOS：F13–F20、Help、Insert、小键盘回车键映射；急停快捷键 `F13` / `⌃⌥⌘P`；连续滚动（触控板）按点增量聚合阈值触发。
- 便携版打包：`Mouse.Insight_<ver>_portable.zip`（含 `data/.portable` 标记）。
- CI：`Swatinem/rust-cache`、`cargo test --locked`、`check-release-version` 一致性检查、`rustsec/audit-check`（Windows job）。
- NSIS 卸载钩子：清理自启注册项与日志目录（保留 `config.json`）。
- 辅助功能：ARIA live 区域、`prefers-reduced-motion`、`forced-colors` 支持、明暗主题告警变量。

### Fixed

- 引擎 worker panic 后按键残留按下（P0-1）。
- `reset_all` 发送失败被静默吞掉（P0-2）。
- 急停依赖可能阻塞的 worker 线程（P0-3）。
- `RegisterHotKey` 失败无反馈，且钩子内无兜底急停路径（P0-4）。
- 前端 `invoke` 无超时、无失败提示（P0-5、P0-7）。
- 配置解析一处字段损坏即整文件丢弃；现逐字段恢复，损坏映射条目单独跳过，未知字段保留（P0-6、P1-17、P1-18）。
- 配置文件写入非原子（Windows 下崩溃可致半截文件）：改 `MoveFileExW` 原子替换，损坏副本最多保留 5 份（P1-31）。
- 暂停状态运行时与磁盘不一致：`set_paused` 先应用运行时、持久化失败返回错误（P1-35、P1-36）。
- `SendInput` 部分注入成功时引用计数失衡，改用待释放队列补偿（P1-21）。
- 录制时物理按键状态初始化存在 TOCTOU（P1-23）。
- 钩子状态仅在轮询时读取，故障前端无感知（P1-10/11/12）。
- 边沿队列溢出导致的暂停不再写盘（区分瞬时/刻意暂停）（P1-41）。
- X-Mouse Button Control 检测在进程枚举失败时误报「未运行」，改为缓存上次结果并带 TTL（P1-45）。
- 前端识别/录制竞态：stale 事件、保存队列乱序、窗口失焦未撤防、双击确认等，改用 generation 计数 + 保存串行化（P1-5..P1-46 前端项）。
- 主键误吞、左右键结束识别、录制无超时、失焦残留录制键（多项 P1/P2）。
- `dual + keys` 旧配置静默失效：现迁移为 tap 行为；清空 dual 需三个键槽全空。
- 切换模式/按键误清另一键槽数据；滚轮绑定丢失 hold 槽。
- macOS：反斜杠键映射错误；`active_modifiers` 误存非修饰键；`TapDisabledByUserInput` 不再盲目重启用而是提示权限被撤；runloop 引用改 `Mutex` 存储 + `CFRunLoopWakeUp` 唤醒；修复停止-先于-runloop-发布的竞态（P1-1..P1-47 mac 项）。
- 滚轮：高精度滚轮小 delta 累积后按 WHEEL_DELTA 步进；横向滚轮事件不再误触发。
- 更新检查：区分 404（无新版本）/ 403（限流）/ 超时，不再统一报「检查失败」。
- `open_url` 命令注入面：白名单精确匹配 + 系统绝对路径调用 `cmd.exe`/`explorer.exe`（P2-sec-*）。
- 第二实例带 `--autostart` 不再弹窗；`--quit` 单实例转发正常工作。
- `quit_app` 不再在引擎关闭时阻塞 IPC 主线程。
- CSP 收紧，移除未使用域；`object-src 'none'; base-uri 'none'; form-action 'none'`。
- 脚本：`package-release.ps1` 多处健壮性修复（git 输出解析、超时、相对 target dir、快捷方式策略、点号命名发布产物）；`check-release-version.mjs` 修复 lockfile 解析。
- 大量术语/UI 统一：前侧键/后侧键、短按/长按/滚动、全角标点、「LShift」→「Left Shift」等。

### Changed

- 版本要求：Node `>=22`（原 `>=24`）。
- 移除 `src-tauri/icons/{android,ios}`（桌面应用未引用）。
- 移除 `DraftSession` 及若干死代码；`decide_swallow` 抽为双端共用纯函数。
- 测试：Rust 71→78、前端 41→48；`npm test` 现含 tsc、版本一致性检查与 cargo test。

### Known limitations（需实机验证，见 MouseInsight.md）

- P1-22 Windows 按键掩码顺序、P2-eng-12/13/25 macOS 实机行为、P2-menu-2、P2-ui-9 旧版 WebView2、P2-first-5 macOS stapler。
- P2-eng-17/18/19（macOS 事件回发返回值/批量注入/第三方注入事件）经评估维持现状。
- P2-sec-2 `cmd_tx` 保持无界（EmergencyStop 不允许丢弃），记为接受风险。
