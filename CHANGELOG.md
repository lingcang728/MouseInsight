# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-09-22

全量修复与性能体验重构版本：彻底修复鼠标侧键无法结束命令的底层缺陷，根除按键注入磁盘阻塞与心跳误杀，全面重构 UI 美观度与录制交互，支持完整便携免安装版。

### Added

- 录制弹窗快捷预设：新增显式「中断命令 (Ctrl+C)」、「关闭标签 (Ctrl+W)」、「新建标签 (Ctrl+T)」、「刷新 (F5)」、「全屏 (F11)」、「显示桌面」等快捷预设，候选区补充 F5/F11/方向键/Enter/Backspace/Del。
- 录制弹窗一键清空按钮，点击预设时自动重置当前组合，避免杂键拼接。
- 规范化虚拟键码转换函数（`generic_vk`），标准统一 `VK_CONTROL`、`VK_SHIFT`、`VK_MENU`，同时精确保留硬件扫描码与扩展标志位。
- 静态单通道异步按键日志写入队列（`JOURNAL_TX`）与单线程合并写入（Coalescing），高频触发时零阻塞主线程。

### Fixed

- 鼠标侧键无法结束命令：修正控制台子系统依赖的标准 `VK_CONTROL` 虚拟键注入，解决 Windows Terminal、PowerShell、cmd 无法响应 Ctrl+C 中断信号的问题。
- 放行驱动模拟侧键：底层钩子移除对 `LLMHF_INJECTED` 的无条件拦截，仅比对软件自身签名，兼容雷蛇/罗技等鼠标宏驱动。
- 侧键 XBUTTON 消息位掩码容错（支持 `(xhi & 1) != 0` 与 `(xhi & 2) != 0`）。
- 消除注入核心路径的同步磁盘 I/O 阻塞与无界线程创建风暴。
- 修复 60 秒心跳检测在鼠标点击但光标未位移时误判键盘钩子死亡并频繁重置按键账本的缺陷。
- 全面重构 `<select>` 下拉菜单样式（现代圆角、定制 SVG 箭头、平滑 Focus 光晕、优雅 disabled 态）。
- 按钮触控动效全面加速（缩短至 60ms~100ms），录制弹窗升级毛玻璃背景与聚焦发光边框。

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
- 进程被杀/panic 后注入键残留系统层、重启播种把幻影键当物理按住永不释放：新增 `held-keys.json` 预写日志，启动时先于物理播种补发 key-up；panic 钩子 `try_lock` 兜底释放（P1-1、04-F1）。
- 钩子线程 panic 后 `HOOK_TID` 残留、`hook_status` 停在 ready、`retry_hook` 空转：hook 线程外裹 `catch_unwind`，panic 时清零标识、上报状态并释放注入键（E7、04-F12）。
- 快照 `paused` 改用运行时原子量：队列溢出/看门狗等未持久化暂停不再在前端显示为「已启用」（04-F2）。
- 配置加载校验：非法 `mode` 归位为 hold 并提示；同鼠标键重复映射保留首条；非布尔 `paused`/`autostart` 记录恢复说明（04-F3/F14）。
- `.bak` 仅在现有 `config.json` 可解析时原子更新：从备份恢复后的首次保存不再用损坏文件覆盖唯一好备份（04-F4）。
- 急停持久化线程加 `catch_unwind` 且 `EMERGENCY_SAVE_PENDING` 必清位；退出时短暂等待在途急停保存（04-F5）。
- `save_mappings` 前端超时后不再直接回滚：invoke 落地时用 `get_snapshot` 对账，三方状态收敛（04-F13/前端 F2）。
- `schema_version` 保存不降级更高版本；Mapping 级未知字段随 `extra` 往返保留（04-F6/F7）。
- `config.json` 符号链接写穿透至真实目标、`.bak` 链接先解除；启动清扫 `config.json.*.tmp`/`held-keys.*.tmp`/`.write-test` 残留（04-F9/F10）。
- 便携标记后补/移除导致配置位置切换时给出旧配置位置提示，不自动迁移不删除（04-F15）。
- `logs/crash-*.log` 上限 5 份（04-F11）。
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

- 版本要求：Node `>=22.20`（原 `>=24`）。
- 移除 `src-tauri/icons/{android,ios}`（桌面应用未引用）。
- 移除 `DraftSession` 及若干死代码；`decide_swallow` 抽为双端共用纯函数。
- 测试：Rust 71→93、前端 41→48；`npm test` 现含 tsc、版本一致性检查与 cargo test。

### Known limitations（需实机验证，见 MouseInsight.md）

- P1-22 Windows 按键掩码顺序、P2-eng-12/13/25 macOS 实机行为、P2-menu-2、P2-ui-9 旧版 WebView2、P2-first-5 macOS stapler。
- P2-eng-17/18/19（macOS 事件回发返回值/批量注入/第三方注入事件）经评估维持现状。
- P2-sec-2 `cmd_tx` 保持无界（EmergencyStop 不允许丢弃），记为接受风险。
