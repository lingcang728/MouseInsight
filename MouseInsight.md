# Mouse Insight 全量审计报告

- **项目**：Mouse Insight `0.2.0-beta.6`（Tauri 2，Rust 后端 ~4.7k 行 + 原生 TS 前端 ~1.9k 行，Windows/macOS）
- **审计方式**：29 个只读子代理专项报告 + 主会话直查 9 个小面，全量通读 `engine.rs` / `macos.rs` / `lib.rs` / `native_menu.rs` / `main.ts` / `logic.ts` / `index.html` / `styles.css` / 配置与脚本 / 文档。
- **实测验证（本次已跑）**：`npm run build` ✅ 干净通过；`node tests/frontend_logic.test.mjs` ✅ 41/41；`cargo test` ✅ 52/52。
- **标注约定**：✅confirmed = 代码已逐行核实可复现路径；🟡likely = 静态推断合理但未实机验证；❓needs-verify = 需 Windows/macOS 真机确认。
- **去重说明**：多份报告重复命中的条目已合并，"重复命中"列在每条末尾。共 **43 项独立审计**（34 子代理报告 + 9 项主会话直查，含 cargo audit/git/实测验证），去重后 **~135 条独立问题**。
- **2026-09-12 二次核实**（详见 `docs/audit-verdicts.md`）：对全部 🟡/❓/P3/测试缺口条目逐条对照当前代码复核，✅ 条目抽查位置。结果：**剔除 2 条**（P2-bld-4 高 DPI：tao 默认已 PER_MONITOR_AWARE_V2；P3 "app menu 中英混排"：by-design），**修正 2 条**（P1-42 孤儿自启项机制有误，P2-doc-2 minimumSystemVersion 子项有误），**7 条静态无法定论保留为 ❓真机**（P1-22 / P2-eng-12 / P2-eng-13 / P2-eng-25 / P2-menu-2 / P2-ui-9 / P2-first-5），其余全部 CONFIRMED。

---

## P0 — 卡死/丢数据/安全兜底失效（最优先修）

### P0-1. worker 线程 panic → 已注入按键永久卡死，UI 仍显示"已启用"
- **位置**：`src-tauri/src/engine.rs:1604-1627`（mi-worker 无 catch_unwind）、`macos.rs:410-411`（`CGEventSource::new(...).expect`）、`engine.rs:1125` `engine().expect`
- **场景**：worker 内任何 panic（macOS CGEventSource 创建失败、未来代码回归）→ 所有 `cmd_tx.send` 静默失败 → Pause/ScrollLock 急停无人消费 → **按住不放的修饰键永远不释放**，`hook_status` 停在 "ready"，前端显示一切正常。
- **修法**：worker_loop 外包 `catch_unwind` + panic 路径强制 `reset_all` + `set_hook_status("engine stopped")` + emit 事件；`expect` 全部改优雅降级。
- **验证**：✅confirmed（panic 路径代码级确认；macOS 触发概率需真机）。重复命中：panic/failsafe/error-handling/security/macos-2/logging 六份报告。

### P0-2. `reset_all` 的 send_keys 失败被 `let _ =` 吞掉 —— 卡键零上报
- **位置**：`src-tauri/src/engine.rs:944-949`
- **场景**：暂停/急停/改配置/退出时释放注入键失败（SendInput 部分写入、UIPI）→ key_refs 已 drain、无重试、无 SendReport → 用户键盘"坏了"且无任何提示。这是全仓唯一不上报的 send 路径。
- **修法**：复用 `on_send_error` 回调上报；UI 提示"可能有按键卡住，请按一下对应修饰键"。
- **验证**：✅confirmed。重复命中：error-handling/engine-perf。

### P0-3. SendInput 阻塞时 EmergencyStop 失效（自锁死局）
- **位置**：`src-tauri/src/engine.rs:2243`（worker 内同步 SendInput）、`1096-1104`（edge 溢出→EmergencyStop）
- **场景**：接收方输入队列被挂起前台应用占用 → SendInput 长时间阻塞 → edge 队列溢出触发的 `EmergencyStop` 发给同一个卡住的 worker → **永远不会被执行** → held 键保持按下直到进程退出。
- **修法**：注入键集合放共享状态；急停由独立线程的第二 injector 直接发 key-up 兜底，不依赖卡死的 worker。
- **验证**：🟡likely（SendInput 阻塞是已知 Windows 行为，严重度需真机）。

### P0-4. `RegisterHotKey` 失败静默 —— README/UI 承诺的"无条件急停"可能不存在
- **位置**：`src-tauri/src/engine.rs:1789-1800`；承诺文案 `index.html:96`、`README.md:32,145`
- **场景**：其他程序已占用 Pause/ScrollLock 或安全软件拦截 → 注册失败被 `let _ =` 吞 → 急停键静默不存在，UI 仍写"可紧急暂停"。`kbd_proc` 无 VK_PAUSE 兜底检测。
- **修法**：检查返回值，失败时 set_hook_status 告警 + kbd_proc 加 Pause/ScrollLock 键码兜底。
- **验证**：✅confirmed（失败静默属实；占用概率取决于用户环境）。重复命中：failsafe/docs/error-handling/logging/security 五份。

### P0-5. 录制命令无窗口可见性门控 —— webview JS 可全局吞键采集
- **位置**：`src-tauri/src/lib.rs:53-70,108-111`（`arm_record`/`take_record_keys` 无校验）、吞没逻辑 `engine.rs:1890-1918`、`macos.rs:583-596`
- **场景**：窗口隐藏时 `invoke("arm_record")` → 全局键盘被吞且 chord 可读回。恶意脚本/devtools 篡改场景下的键盘采集原语。另有同族问题：`recording` 若经 IPC/失焦竞态卡 true（见 P1-4），全局键盘持续被吞，仅物理 Esc 可自救。
- **修法**：`arm_record`/`take_record_keys` 加 `window_visible` 与录制遮罩状态门控；engine 侧给 recording 加 idle 超时兜底。
- **验证**：✅confirmed（缺门控属实）。重复命中：security/async-races。

### P0-6. `cfg.write()` 持锁横跨 fsync+MoveFileExW —— 磁盘卡顿冻结整个主线程
- **位置**：`src-tauri/src/engine.rs:1346,1382-1389,1432,1445,1455-1460,1465,1742`（全部在 `cfg.write()` 内调 `save_config`）；读锁方在主线程 `snapshot()`/`menu_summary()`（1357/1332）
- **场景**：杀软锁文件/OneDrive 网络盘/慢盘 → `sync_all` 或写穿 rename 卡数秒 → 主线程 `cfg.read()` 全堵 → **Wry 事件循环冻结**（窗口不可拖动、IPC 全停）+ 前端 invoke 永久挂起。
- **修法**：clone→锁外落盘→拿锁提交；`snapshot` 的 `xmbc_running()`（全进程枚举）改缓存；读侧用 `try_read` 兜底。
- **验证**：✅confirmed（锁内 IO 结构属实；卡顿概率取决于磁盘环境）。重复命中：hang-freeze/config-persist/error-handling。

### P0-7. 前端 `saveQueue` + 18 处 invoke 零超时 —— 一次后端挂起 = 保存链永久瘫痪
- **位置**：`src/main.ts:383-397`（saveQueue 串行链）、全文 invoke 无 `Promise.race` 超时
- **场景**：任一 invoke 永不 settle（P0-6 磁盘冻结、IPC 断链）→ saveQueue 死 → 后续 persist 全排队 → "正在保存…"永转、遮罩滞留、按钮永久 disabled（`btn-pause` 771-787、`record-ok` 947-953）。
- **修法**：统一 `invokeTimeout`；persist 超时后断链自愈。唯一带超时的 `latestRelease`（updates.ts:38 AbortController）可作为模板。
- **验证**：✅confirmed。重复命中：hang-freeze/async-races/maints 三份。

### P0-8. 丢失 mouse-up → 注入键永久卡死 **且该键映射永久失效**（无 watchdog）
- **位置**：`src-tauri/src/engine.rs:659-665`（`next_deadline` 只统计 tap/dual）、`844-846`（`active_holds.contains_key` 直接 return）、`1668-1690`（仅 hold/toggle 存活时 `Duration::MAX` 无限阻塞，无周期 tick）
- **场景**：丢 up 的四条真实路径——按住时鼠标拔出/蓝牙断连、按住期间系统睡眠（S3 无输入事件）、Win+L/UAC 安全桌面/RDP 切换（LL 钩子收不到对端桌面输入）。up 丢失后：① `active_holds` 残留 → 注入的 Ctrl/Alt 永远按住（用户键盘全面错乱）；② **下次再按该物理键 `contains_key` 命中直接 return → 该键映射永久失效**直到手动暂停/恢复；③ `swallowed_buttons` 位同样残留（见 P1-12）。
- **修法**（一个机制覆盖全部丢-up 场景）：worker select 超时封顶 1-2s，tick 时对 `active_holds`/`active_toggles`/`pending_dual` 中的按键用 `GetAsyncKeyState`（mac：`CGEventSourceButtonState`）实测物理状态，物理已松开则强制 release 并清 swallow 位。
- **验证**：✅confirmed（无 watchdog 结构属实；丢-up 概率取决于场景频率）。重复命中：edge-cases 专项 P1-1 升级原 P0-8。

### P0-9. 无任何日志/崩溃兜底 —— 生产环境故障完全不可诊断
- **位置**：全仓无 `log`/`tracing` 门面（`Cargo.toml`）、无 `panic::set_hook`、`main.rs:1` `windows_subsystem="windows"` 使 8 处 `eprintln!` 全落虚空
- **场景**：钩子失败/注入失败/配置损坏/worker 死亡/急停热键注册失败 —— 终端用户机器上零痕迹，issue 只能靠口述。`docs/feedback-2026-09-08.md:7` 的"无法复现"正是此果。
- **修法**：`tauri-plugin-log` 写 `config_dir()/logs/`（3×1MB 滚动）；panic hook 写 `crash-<ts>.log`；托盘加"导出诊断信息"；**只记事件级日志，永不记按键内容**（README:156 隐私承诺红线）。
- **验证**：✅confirmed。重复命中：logging/error-handling/failsafe。

---

## P1 — 功能错误（确认会导致错误行为）

### 平台正确性

**P1-1. macOS 无任何键盘急停途径** ✅confirmed
- `macos.rs:559-599` 键盘分支无 panic 键；README:32,145 未标平台限定（`main.ts:625` 应用内已改口"可从托盘暂停"，间接承认缺失）。
- 修：CGEventTap 键盘分支加等价急停（如 F13/⌃⌥⌘P，置于录制分支之前）；README 注明 Pause/ScrollLock 仅 Windows。重复命中：macos-2/platform-parity/docs 三份。

**P1-2. `macos.rs:193` `"\\\\"` 是双反斜杠笔误** ✅confirmed
- 单反斜杠 token `"\"` 在 mac 上 `key_spec` 返回 None → `set_mappings` 拒存/加载静默丢 chord；Windows `engine.rs:2144` 是 `"\\"` 正确。改 `"Backslash" | "\\"`。重复命中三份。

**P1-3. macOS 键表缺 Insert / F13-F24 / NumpadEnter / CapsLock** ✅confirmed
- `macos.rs:161-266` 仅 F1-F12 字面量、无 Insert（可用 kVK_Help=0x72）、无 `F<num>` 通用解析、`kVK_ANSI_KeypadEnter=0x4C` 常量已定义未接线；`keycode_to_token` 反向表同缺 → Mac 上物理无法录制、从 Windows 迁移来的 config 无法保存任何修改（含未知键 chord 被 `set_mappings` 拒）。
- 修：补齐双向表 + `F<num>` 兜底；`modifier_weight`（engine.rs:1031-1039）同步补 mac 别名权重（`Cmd|Command|LCommand|RCommand`→40、`Option|LOption|ROption`→30），否则手改配置 chord 注入次序颠倒。重复命中四份。

**P1-4. macOS 触控板/平滑滚动：要么风暴要么不触发** 🟡likely
- `macos.rs:627-636` 只读 `POINT_DELTA_AXIS_1`：触控板逐像素连续事件 → 每个非零 delta = 一次完整 click → 可打满 edge 队列（cap 256）触发非预期 EmergencyStop；纯 pixel-delta 设备 delta=0 → 永不触发。附带 macOS 已知 bug：Shift+滚轮 PointDelta 不移轴 → 横向滚动被误判纵向吞掉。
- 修：读 `IS_CONTINUOUS`/`FIXED_PT_DELTA` 兜底 + 时间窗聚合/累积阈值。

**P1-5. `Focused(false)` disarm 与 `arm_*` IPC 竞态 → 引擎卡死态** ❓needs-verify
- `lib.rs:281-285` 失焦直接 disarm，与 `invoke("arm_listen")`/`arm_record` 同主线程但来源不同、顺序不保证 → `listening`/`recording` 可卡 true（recording 卡死 = 全局键盘被吞，见 P0-5 后果面）。修法：合并 `set_listening(bool)` 带 generation，或 arm 返回后复查。

**P1-6. 托盘 quick mapping 与 UI persist() 互相整体覆盖 → 静默丢编辑** ✅confirmed
- `engine.rs:1384` `next.mappings = mappings` 整体替换；`main.ts:580-587` `mappings-changed` 的 `await saveQueue` 等不到后到的新 save → 旧快照覆盖回显 → 两端编辑互丢。修法：save 带 base generation 做 CAS，或事件 payload 直接带最新 mappings。重复命中三份。

**P1-7. boot 事件丢失窗口 → UI 暂停态与引擎永久不一致** ✅confirmed
- `main.ts:588` `get_snapshot` → `673-762` 七个 `await listen` 之间隔 persist+动态 import 几十~几百 ms → 期间 Pause 急停/`injection-error`/`runtime-binding-changed` 永久丢失（Tauri event 无 replay）。修法：listen 全部移到 snapshot 前注册，或注册后重拉快照对账。

**P1-8. 窗口重开后 `runtimeStates` 丢失 → toggle/hold pill 反显** ✅confirmed
- `main.ts:70,702-709`；`Snapshot`（engine.rs:393-401）无 active 状态字段；`runtime-binding-changed` 被 `WINDOW_ACTIVE` 门控。toggle 保持中关窗重开 → UI 显示"已关闭"，引擎键仍按着。修法：Snapshot 加 `active_bindings` 或可见时重放。

**P1-9. 「取消识别」按钮点击自己被捕获 → 取消变重开** 🟡likely
- `engine.rs:2011`/`macos.rs:640` 对含左右键的所有按键 `listening.swap(false)`；事件先于 DOM click 落地 → click handler 走 else 分支重新 arm → 可能"永远取消不掉"。修法：捕获只对非 primary/非 wheel 生效（`!is_primary && !is_wheel`）。

**P1-10. `checkHook` 无 catch + 超时后永不重查 + hook_status 不推送** ✅confirmed
- `main.ts:565-577` invoke 裸 await、10×500ms 后定格"监听需要处理"无重试入口；`set_hook_status`（engine.rs:1339-1342）只刷托盘不 emit → 钩子 ready 晚到/运行中死亡窗口永久显示错误态；且 `engine-state-changed` 会把"需要处理"覆盖回"正在检查监听"自相矛盾。修法：try/catch + `hook-status-changed` 事件 + 三态区分。重复命中五份。

**P1-11. Windows 钩子被系统静默摘除零检测** ✅confirmed
- LL hook 因 `LowLevelHooksTimeout` 被摘除无通知；`hook_loop` 只覆盖创建失败和 `GetMessageW=-1`（engine.rs:1822）→ 映射静默失效到重启。修法：worker/独立线程 30s 心跳 `PostThreadMessageW` 探活 + 摘除后自动重建。重复命中：logging/platform-parity/perf。

**P1-12. `swallowed_buttons` 位图无任何 reset 点** ✅confirmed
- `engine.rs:1085,2044-2046`、`macos.rs:673-675`：仅 up 路径清除；tap 禁用窗口期丢 up → 残留 bit 吞掉下一次正常点击的 up（目标应用只见 down 不见 up）。修：`ResetState`/`EmergencyStop`/`SetPaused` 时 `store(0)`。

**P1-13. `listenTimer` 泄漏：arm pending 期取消后旧 timer 掐掉下一轮识别** ✅confirmed
- `main.ts:789-802`：await `arm_listen` resolve 后无条件 `setTimeout`；期间 `stopListening` 清不掉未来 timer → 15s 后误杀新会话。修法：设 timer 前 `if (!listening) return` + generation counter。重复命中：async-races/maints-3。

**P1-14. `confirmRecord` persist 窗口期内取消无效 → 录的键照样落盘** ✅confirmed
- `main.ts:508-563`：`currentDraft !== draft` 只查 take_record_keys 后一次；`await persist()` 期间 Esc/blur/遮罩点击走 closeRecord 但 confirm 仍写映射。修法：persist 前后各查一次，或先 closeRecord 再保存。重复命中两份。

**P1-15. 删除映射无确认无撤销，立即落盘** ✅confirmed
- `main.ts:896-906`：点删除即 `filter`+`persist`，无 confirm/undo/toast；"删除"与"折叠"并排易误点。修法：复用 `confirmedMappings` 做 5 秒可撤销删除。

**P1-16. `showAlert` 横幅只增不减永久驻留** ✅confirmed
- `main.ts:440-446` + `index.html:31`：无 hideAlert/自动消退/关闭按钮；瞬时错误（按了一下左键、UIPI、保存失败）挂到关窗；且失败文案与随后"已保存"并存自相矛盾。修法：非致命 5-8s 自动消退 + 成功路径清除。重复命中六份。

**P1-17. 单字段非法 → 整份配置重置且零感知** ✅confirmed
- `engine.rs:338-348` Mapping 必填字段无 `#[serde(default)]` + `1194-1197` 全有或全无解析：任一 mapping 缺 `keys`/类型错/尾随逗号 → 走 `.bak` → 再失败 `AppConfig::default()` 全部映射清零；恢复事件只进 `eprintln`（release 不可见）；用户在"空配置"上再编辑会覆盖原 config。修法：`Value` 逐条 salvage 坏条目；Snapshot 加 `config_recovered` 字段让前端弹 banner。重复命中：config-persist/logic-ts/error-handling。

**P1-18. 单个未知 key token 静默作废整条 chord + 加载路径零校验** ✅confirmed
- `engine.rs:256-257` `specs_from_names` 遇一坏 key 丢整 chord → `322-324` 静默跳过映射，UI 无任何标记；`load_config` 不经 `set_mappings` 校验。修法：跳过单个坏 key 而非整 chord；加载时校验并回传前端标红。重复命中：logic-ts/tests/error-handling。

**P1-19. 自启链路：便携移动后静默失效 UI 假勾选 + Run 键无引号 + `--autostart` 不识别** ✅confirmed
- `auto-launch` `is_enabled()` 不校验登记路径=当前 exe → exe 移动后 UI 仍显示已勾选且回写 config（main.ts:603-613）。`tauri.conf.json:4` `mainBinaryName:"Mouse Insight"` 含空格，上游 `auto-launch-0.5.0/windows.rs:42` `format!("{} {}", app_path, args)` **已查源确认不加引号**；`is_enabled()`（73-83）只查值存在+StartupApproved 标志、不比对路径。注册表值名为 `HKCU\Run\Mouse Insight`（productName，非 package.json 名）。`lib.rs:256-263` 单实例回调只认 `--quit`，第二自启入口会弹主窗违反"静默托盘"承诺。修法：boot 时幂等 `enable()` 刷新路径；回调加 `argv.contains("--autostart") return`；上游 issue/自写注册表。

**P1-20. 更新检查当前可用但依赖"beta 不标 prerelease"的发布习惯** 🟡（已实测修正）
- 实机核实：远端 `gh release view v0.2.0-beta.6` → `isPrerelease:false`、`isDraft:false`、被标记为 **Latest** → `/releases/latest` 正常返回，更新检查**目前工作正常**。原报告"必然 404"的判断不成立——前提是发布时一直手动把 beta 标为 Latest。若未来某次 release 勾选 "Set as a pre-release"，该接口即跳过它 → 检查更新静默失效。修法：代码侧对 404 单独处理为"尚未发布更新"；发布流程文档固化"beta 不勾 prerelease"。重复命中：updates/docs。

**P1-21. SendInput 部分插入 → Key Ledger 虚实脱钩** ✅confirmed
- `engine.rs:684-700`：`key_refs` 在 send 前 +1；inserted<expected 时未按下的键被记为持有 → 同键不再补发、后续可能发幻影 key-up；release 失败方向（P0-2）同根。修法：按 inserted 数回滚计数。

**P1-22. `send_mask` 时序疑似错误（Alt/Win 释放后被吞/开始菜单抢焦）** ❓needs-verify
- `engine.rs:721-734,944-949`：mask 在修饰键 key-up **之后**发送；主流实现是掩码键与修饰键 up 同批或在 up 前插入 dummy key。Windows 真机验证 Alt 释放后焦点是否被开始菜单抢走。2026-09-12 复核：结构属实，行为需真机；**本轮不改时序**（无实测依据改错更糟），仅保留条目。

**P1-23. 物理键状态镜像三处失真源** 🟡likely
- (a) Windows `physical_down_set` 只由运行期 kbd_proc 填充 → 启动前已按住的键漏检 → `release_specs` 误发 key-up 打掉用户真按着的键（mac 用 `CGEventSourceKeyState` 实时查反而更好）；(b) `is_physical_down` 同键连调两次 TOCTOU（engine.rs:709/712）；(c) mac FlagsChanged 方向靠实时查询而非事件自身 flags（macos.rs:564-568）。修法：Windows 启动用 `GetAsyncKeyState` 播种；取一次快照复用。

**P1-24. 识别 15s 超时零反馈 + 失焦静默丢录制** ✅confirmed
- `main.ts:797` 超时仅复位文案；`lib.rs:281-285`+`main.ts:1046-1049` 失焦 `closeRecord` 丢 `recordBuf` 无任何提示（用户切去验证回来录制内容没了）。修法：超时显示"15 秒内未检测到按键"；失焦取消且缓冲非空时重聚后提示。重复命中两份。

**P1-25. 切换模式/按键静默丢已录键** ✅confirmed
- `main.ts:859-876` toggle 切换清空双槽（hold 槽内容无声吞）；`853-857` 改滚轮清 hold_keys 不可恢复。修法：将丢弃非空槽位时弹确认或保留数据。

**P1-26. `preset` 录制是"替换"语义：静默清空已录键** ✅confirmed
- `main.ts:827-828` 先 `arm_record`（后端 `recorder.reset()`）再逐键 add → 手录一半点预设全丢；chips 语义却是追加，不一致。修法：preset 走 `add_record_key` 追加或 UI 明示"填充"。重复命中：logic-ts/maints-1。

**P1-27. macOS 录制吞掉全部键（含不可 token 化的）与 Windows 相反** ✅confirmed
- `macos.rs:583-596` 录制态除 Tab 外无条件 Drop；Windows `engine.rs:1907-1912` 对 `vk_to_token=None` 放行（还能走 DOM 兜底 codeToToken 录 Insert/F13 等）。Mac 上 CapsLock/小键盘回车/F13+ 录制期被静默吃掉且前台应用修饰态漂移。修法：`keycode_to_token==None` 时 `return Keep`。

**P1-28. `boot()` get_snapshot 失败 → 整页半死无重试** ✅confirmed
- `main.ts:1066-1068`：只弹 banner，renderMaps 未跑、按钮全裸奔；且 listen 已注册一半。修法：catch 里渲染错误态 + "重试"按钮。

**P1-29. `mappings-changed` 监听器竞态可让已删映射在 UI"复活"** ✅confirmed
- `main.ts:580-587` 详见 P1-6 同族：旧快照覆盖 `mappings` 与 `confirmedMappings`，回滚基线也错。修法同 P1-6。

**P1-30. autostart 勾选：失败回滚方向撒谎 + 无防重入** ✅confirmed
- `main.ts:1006-1016`：(a) `import()` 在 try 外失败 → 不回滚；(b) enable 成功但 `save_autostart` 失败 → 复选框回滚谎称未自启（OS 已启）；(c) 无 busy flag 连击乱序；(d) 上游 `disable()` 对不存在条目报 NotFound → 勾选被弹回。修法：catch 里重读 `isEnabled()` 回填 + showAlert + 复用 saveQueue 串行。重复命中：autostart/maints-1/error-handling。

**P1-31. `set_paused` 先改内存后落盘，失败文案暗示"没生效"** ✅confirmed
- `engine.rs:1344-1364,1453-1461`：保存失败时引擎已实际暂停/恢复，文案"暂停状态保存失败"误导；磁盘与内存从此分叉到重启。修法：文案改"已生效但未写入配置，重启后恢复"或返回 `{applied, persisted}` 结构。

**P1-32. `update` 按钮：`app_version` 失败则永久失效** ✅confirmed
- `main.ts:1054-1064`：click 绑定在 invoke 成功 then 里，失败则按钮裸奔无提示。修法：catch 兜底绑定或 handler 上移。

**P1-33. `is_modifier` 别名集前端 ⊋ 后端 → "Cmd" 整 chord 死** ✅confirmed
- `logic.ts:59-86` 认 26 个别名（cmd/command/meta/lmeta…），`engine.rs:1031-1039` + `win_key_spec` 只认 canonical 大小写敏感 → `inferDefaultMode` 推 hold 但 `key_spec("cmd")=None` → 叠加 P1-18 整 chord 失效。修法：共享一份 canonical token 表。

**P1-34. `quit_app` 同步 command 卡主线程 ≤800ms + 托盘 accelerator 死文案** ✅confirmed
- `lib.rs:142-145` sync command 在 UI 主线程跑 `recv_timeout(800ms)`（托盘路径已 spawn_blocking 正确）；改 async。`native_menu.rs:127-135` 托盘项挂 `CmdOrCtrl+…` 加速键在 Windows 托盘 popup 不派发（muda 只对窗口菜单栏分发）且与真急停键 Pause/ScrollLock 文案误导。重复命中：native-menu/hang-freeze/platform-parity。

**P1-35. toggle_paused 读-改-写非原子** ✅confirmed
- `engine.rs:1344-1352` `!paused.load()` 两次 load 可同值 → 连点两次只翻一次 + save_config 跑两遍；更严重：与 edge 溢出的 `paused.swap(true)`/EmergencyStop 交错时，**一次菜单点击可能把紧急保护撤销回未暂停**。修法：`fetch_not` 或菜单命令携带显式目标状态。

**P1-36. 空映射残留 + 计数失真 + 托盘"未设置"显示错误** ✅confirmed
- `engine.rs:1393-1427` quick 清除后 `keys/tap/hold` 全空的 Mapping 永久残留 config；`menu_summary` 按 `cfg.mappings.len()` 计数 → 托盘显示"1 个按键"实际零生效；`native_menu.rs:45-47` 不读 legacy `m.keys` 回退 → 旧配置托盘显示"未设置"但实际可触发。

**P1-37. `Snapshot.listening`/`initialKeys`/`DraftSession` 死代码链 → 前后端状态不同步窗口** ✅confirmed
- `main.ts:44` 类型有 `listening` 但从未消费：webview 重建时后端 listening=true 前端 false，下一次非主键按压被吞。`initialKeys` 写入零读取 → "重录"总是从空开始、取消不可还原。`DraftSession`（logic.ts:327-449）整个类未被 import 却占了测试 suite 4 九个用例（详见测试缺口）。

**P1-38. `listen-captured` 晚到 → 用户取消后仍弹录制窗** 🟡likely
- `main.ts:724-727` 事件经 cmd_tx→worker→emit 有延迟，`stopListening` 无法撤回在飞 emit → `!currentDraft` 时走 openRecordForNew 弹遮罩。修法：handler 校验本地 `listening` 标志/epoch。

**P1-39. `Escape` 录制 down/up 吞放不对称** ✅confirmed
- kbd_proc 吞 Esc down 取消录制但 Esc up 放行 → 对端应用收到孤儿 up。取消录制时按住其他键也有同类不对称。

**P1-40. 前端兜底 keydown 把物理键加成永久 chip → 顺序按 C、V 录成 C+V** ✅confirmed
- `main.ts:996-1001` + `engine.rs:998-1020` `add_chip` 写入不消退的 `chip_modifiers`；钩子失效时这是唯一录制路径，累积行为错误。修法：区分 chip 与物理捕获键。

**P1-41. EmergencyStop 不解除 `recording` —— 急停后键盘仍被全局吞掉** ✅confirmed
- `engine.rs:1734-1748`（EmergencyStop 不含 recording 处理）+ `1890-1917`（recording 时所有可录键 `LRESULT(1)`，不看 paused）：录制中按 Pause/ScrollLock 或 edge 溢出触发急停 → 引擎已暂停但键盘全局仍被吞（除 Esc/Tab），用户以为已恢复实则键盘"失灵"。修法：急停路径同时 `recording.store(false)` + `recorder.reset()` + `RecordCancel`。新发现：edge-cases。

**P1-42. 卸载残留：StartupApproved 孤儿值 + `%APPDATA%\MouseInsight` 不清理 + mac plist 仅删文件** 🟡PARTIAL（2026-09-12 修正）
- ~~原报告"Run 值名 `mouse-insight` 与 NSIS 卸载器不匹配 → 孤儿自启项"**不成立**~~：查 `tauri-codegen context.rs:267-271` + `tauri-plugin-autostart-2.5.1/lib.rs:178-182`，未设 `app_name` 时取 `package_info().name` = **productName "Mouse Insight"**，与 NSIS 卸载器删除名一致。
- 仍成立：① `disable()` 只删 `Run` 值不清 `StartupApproved\Run\Mouse Insight`（auto-launch windows.rs:65-70 vs 46-54）；② `%APPDATA%\MouseInsight` 自定义目录不在 Tauri app-data 路径下，`deleteAppDataOnUninstall` 删不到；③ macOS `~/Library/LaunchAgents/Mouse Insight.plist` disable 仅删文件不 `launchctl bootout`。修法：NSIS hook 清理 StartupApproved + APPDATA 目录；文档注明 mac 残留。新发现：first-run。

**P1-43. 便携 exe 默认并不便携 + 便携标记后补导致"配置丢了"** ✅confirmed
- `engine.rs:1134-1184`：单 exe 下载无 `data/.portable` 标记 → 首启配置写 `%APPDATA%\MouseInsight`；用户后补标记 → `load_config` 不迁移不合并，`data/` 优先胜出 → 用户视角"配置全丢"。README"绿色便携版（推荐）"与单文件分发形态矛盾。修法：发布 zip（含 `data/.portable`）或首启探测可写则启用便携。新发现：first-run。

**P1-44. 便携版 + 缺 WebView2 = 完全静默失败** ✅confirmed
- `lib.rs:214-219` 窗口创建失败仅 `eprintln!`（release 不可见）→ 无 WebView2 的 Win10 LTSC 双击后只有托盘图标或秒退，零提示。修法：窗口创建失败回退 `MessageBoxW` 提示 + 打开下载页。新发现：first-run。

**P1-45. 退出 800ms 超时 → 进程强退留"幽灵按键"** ✅confirmed
- `engine.rs:1521-1548`：`shutdown` 发 `ResetState(Shutdown)` 后 `recv_timeout(800ms)` 超时即 `app.exit(0)` → worker 若在阻塞，注入键保持按下（SendInput down 不随进程死亡复位）。修法：injector 侧维护"已注入未释放"集合，超时后独立兜底全员 key-up。新发现：memory-leak。

**P1-46. 「识别鼠标键」不检查 `hookReady` → 钩子未挂上时静默空转 15s** ✅confirmed
- `main.ts:789-802` 不查 hookReady；`arm_listen` 只置原子标志 → 钩子没挂上时按键无回调，用户干等到 15s 超时复位无错误反馈。修法：前置检查 `hookReady`，未就绪直接 showAlert。新发现：first-run。

**P1-47. macOS 授权流程死端：授权后必须整个重启应用** ✅confirmed
- `macos.rs:481-484` 未授权 → 弹系统框 + 线程退出写死状态；用户在系统设置开权限后无重试/无"我已授权"按钮（入口只在托盘菜单）。修法：banner 内嵌"打开辅助功能设置""重试监听"按钮，重试重跑 `hook_loop`（RUN_LOOP_REF 需改可重置）。新发现：first-run（与 macos-1 #6 合并）。

---

## P2 — 隐患/体验/兼容性（按子系统分组）

### 引擎与输入（Windows/macOS）
- **P2-eng-1**：轮 `WM_MOUSEWHEEL` 不归一化 delta（engine.rs:2090-2097）：delta=0 误判 WheelDown、高分辨率滚轮过触发、大 delta 欠触发。❓
- **P2-eng-2**：`TAP_QUEUE_CAP=8` + tap dwell 30ms：高速滚轮/连点丢触发静默；与"每按一下触发一次"表述不符。✅
- **P2-eng-3**：toggle 无防抖：硬件双击/重复 down 立即 on→off。✅
- **P2-eng-4**：paused 与 mouse-up 竞态：up 在 pause 前后处理可能触发残留 dual/tap 动作。🟡
- **P2-eng-5**：未映射的 mouse-up 也占 edge 队列容量 → 极端下凑满 256 触发急停。✅
- **P2-eng-6**：`HOOK_TID` 早退路径（模块句柄失败 1778、鼠标钩子失败 1782-1786）不复位 → tid 悬空，shutdown 可能向复用 tid 投 WM_QUIT。✅
- **P2-eng-7**：worker `edge_rx` 断连分支 `Err→continue` 是潜伏 100% CPU 死循环（当前不可达因 edge_tx 存 OnceLock，属演进地雷）。✅
- **P2-eng-8**：due deadline 多等 1ms（engine.rs:1668-1678）；telemetry 线程 `recv` 永不退出（telem_tx 存 OnceLock）— 退出卫生。
- **P2-eng-9**：worker 热路径多处克隆 mapping id/spec vec + `emit_state_change` 每事件分配字符串走 IPC — 高频按住时 GC/锁竞争压力。✅
- **P2-eng-10**：`release_specs` 失败不记 pending → 永久失同步（与 P0-2/P1-21 同族）。✅
- **P2-eng-11**：`active_modifiers`（macos.rs:429-430）混入非修饰键死项；mac `physical_down_set` 只写不读（唯一读者是 `#[cfg(windows)]` 的 Win32Injector）——每键盘事件白做一次 RwLock::write。✅
- **P2-eng-12**：`is_physical_down`（mac `CGEventSourceKeyState(HIDSystemState)`）语义按 Apple 文档正确，但若实测合成事件污染 HID 表 → release 永不发 key-up = P0 级。❓**列为真机必验项**。
- **P2-eng-13**：macOS `CFRunLoopStop` 未配 `WakeUp`（macos.rs:474-478）— 阻塞在 mach port 的 runloop 可能延迟退出，app.exit 兜底。❓真机（结构属实，加 `wake_up()` 无害可顺手修）
- **P2-eng-14**：mac `stop_hook`/`RUN_LOOP_REF` set 竞态（macos.rs:533）：set 前调 shutdown → stop 无效 → 线程泄漏。✅
- **P2-eng-15**：`TapDisabled` → EmergencyStop 会持久化 `paused=true` 写盘（engine.rs:1740-1747）：瞬时系统负载导致永久暂停且重启仍停 — 激进取舍，建议区分"运行时暂停"与"持久化暂停"。✅
- **P2-eng-16**：`CGEventTapEnable` 无返回值校验；对 `TapDisabledByUserInput`（用户撤权限）也自动重启用 — 与用户意图对抗。✅
- **P2-eng-17**：mac `event.post(HID)` 返回 `()`，投递失败零观测（对比 Windows SendReport 链路完整）。✅
- **P2-eng-18**：mac 注入逐事件 post 且每事件查 `CGEventSourceFlagsState` — chord 间可被真输入插队；Windows 单批 SendInput 原子性更好（CGEvent 无批量 API，可缓存 flags）。✅
- **P2-eng-19**：Windows 滤所有注入事件（LLKHF_INJECTED），mac 只滤自家 EXTRA_INFO — Karabiner 等第三方注入在 mac 被当物理输入，且 `xmbc_running` 非 Windows 恒 false → Mac 对 Karabiner/Mac Mouse Fix 冲突零提示。✅
- **P2-eng-20**：录制中按 Pause/ScrollLock 触发急停但不解除 `recording` 标志（engine.rs:1890 录制检查不看 paused）→ 急停后录制态仍吞键。✅
- **P2-eng-21**：`menu_summary`/`menu_mappings` 经 `engine().expect`（ENGINE 未初始化即 panic）；托盘 icon `.expect("app icon")`（native_menu.rs:222-224）setup 期硬 panic 点。✅
- **P2-eng-22**：`xmbc_running` 枚举失败谎报 false（engine.rs:1292-1295）→ XMBC 冲突横幅漏显示；且仅 boot 查一次、横幅无"重新检测"按钮。✅
- ~~P2-eng-23~~：**wScan 0xE0 前缀疑点已证伪** —— 实测 `cargo test` 通过（`right_alt_injection_uses_extended_scan_code` 断言 wScan=0x38 + EXTENDEDKEY flag），MAPVK_VK_TO_VSC 不带 0xE0 前缀，编码正确，非 bug。✅已排除
- **P2-eng-23b**（新确认）：Cargo.lock 同时编译 `windows v0.58`（直接依赖，本仓代码使用）与 `windows v0.61.3`（tauri/tao/wry 传递依赖）→ 二进制膨胀，建议升级 `windows = "0.61"` 对齐消除双版本。✅
- **P2-eng-24**：macOS 注入 `spec.extended` 被忽略（mac 键码自带左右侧，无害，仅备注）。✅
- **P2-eng-25**：睡眠/休眠跨 `Instant` 语义分叉：Dual 按住→系统睡 8h→唤醒松开，`Instant` 是否计入 S3 随平台漂移 → 同一操作在 mac 变 hold、在 win 可能仍是 tap；tap_in_flight 跨睡眠则注入键被"按住"整段睡眠。修法：电源恢复通知后统一 `reset_all`（P0-8 watchdog 顺带覆盖）。❓
- **P2-eng-26**：edge 事件无时间戳 → dual 阈值/tap dwell 按"处理时刻"而非"事件时刻"计算：队列积压 300ms 后处理会把物理 500ms 长按判成 tap（单向良性漂移但有感知差异）。修法：`MouseEdge` 携带 hook 侧 Instant。✅
- **P2-eng-27**：mac `TapDisabled` 风暴无去重：tap 反复被禁用时每次回调都发 EmergencyStop + `thread::spawn` 存盘线程（engine.rs:1740-1746）→ 批量短命线程 + `cfg.write` 竞争。修法：pending-save AtomicBool 去重 / worker 内联存盘。✅
- **P2-eng-28**：`key_refs` 计数归零后条目不 remove（engine.rs:702-719）→ 只增不减至 reset；`tap_queue` 弹空后 entry 不删；`collapsedMappings` 折叠后删除映射不清理 id（main.ts:901）——全部有界微泄漏，卫生项。✅
- **P2-eng-29**：mac `app.hide()`（Cmd+H）不清 `window_visible`（lib.rs:224-247,268-287）→ 隐藏时遥测 emit 空转纯耗 CPU/电。修法：挂 hide 回调同步 `set_window_visible`。✅
- **P2-eng-30**：`arm_record` 不从 `physical_down_set` 播种 `physical_held` → 按住 Ctrl 再点开始录制会漏掉 Ctrl（engine.rs:1481-1487）；`RecorderState` 等长 chord 互相替换（983-985）语义需注释固化；`take_record_keys` 清 flag 与读之间有竞态可丢一键（1513-1519）。✅
- **P2-eng-31**：录制期间 LWin 被吞 → Win+L/Win+D 无法锁屏/切桌面（模态设计取舍，建议文案说明）；Pause/ScrollLock 热键只能停不能恢复（溢出暂停后只能走 UI/托盘恢复）。✅

### 配置持久化
- **P2-cfg-1**：`schema_version` 只写不读、无迁移、无版本拒绝；未知字段读宽容但**保存时被永久丢弃**（v2 配置被 v1 打开再保存即丢新字段）。✅
- **P2-cfg-2**：UTF-8 BOM/UTF-16/GBK 手改配置当损坏重置；`try_parse_config` 加一行 `trim_start_matches('\u{feff}')` 即可。✅
- **P2-cfg-3**：`.bak` 备份 `fs::copy` 非原子 + 失败静默；tmp 文件失败路径残留 `config.json.{pid}.{seq}.tmp` 无启动清扫；`.corrupted.*` 无上限累积。✅
- **P2-cfg-4**：便携标记每次调用实时 `is_file()` 探测 → 运行中创建/删除 `data/.portable` 导致 load 读 A 目录 save 写 B 目录，配置分裂 + .bak 错位。修法：启动时 OnceLock 解析一次。✅
- **P2-cfg-5**：跨进程写无协调：单实例互斥是会话级（runas/RDS 可绕，待验证），双实例写同 config 无文件锁 → lost update + bak 内容不定。✅
- **P2-cfg-6**：env 缺失静默降级：`APPDATA`/`HOME` 缺失 → 写 exe 同目录（Program Files 下必败）；`current_exe` 失败 → cwd（快捷方式起始位置决定）。无日志无 UI 提示。✅
- **P2-cfg-7**：便携版放 Program Files 只读目录：无启动期可写性探测，用户配完 5 个映射逐条保存失败；报错是原始 os error 无引导。✅
- **P2-cfg-8**：`MoveFileExW` 目标被占用（杀软/编辑器无共享打开）→ 原始错误码 + tmp 残留；建议对 5/32 错误码给友好文案。✅
- **P2-cfg-9**：macOS rename 后无目录 fsync，断电持久性弱于 Windows 的 WRITE_THROUGH。❓（APFS 风险低）
- **P2-cfg-10**：boot 两处隐式写（sanitize 后自动 persist、autostart 真值回写）+ `JSON.stringify` 脏检查对 key 顺序敏感多写一次盘 — 小。✅
- **P2-cfg-11**：`save_mappings` 对 `id`/`label`/`mode` 无长度/内容校验：devtools 可写 5×N MB 撑爆 config；`mode:"garbage"` 被 `from_str_fast` 静默降级为 Hold 落盘执行。修法：id ≤64 `[A-Za-z0-9_-]`、mode 白名单拒绝。✅（security）
- **P2-cfg-12**：`save_theme`/`save_autostart` 无白名单（theme 任意字符串落盘）。✅

### 前端与异步
- **P2-fe-1**：`saveQueue` catch 块内 `renderMaps`/`structuredClone` 若抛异常 → 队列毒化，之后 persist 全静默不落盘。建议队尾 `.catch(()=>{})`。✅
- **P2-fe-2**：`stopListening` 的 `disarm_listen` 裸 await：reject 会中断 `listen-captured` 后续开录制窗逻辑，捕获的键静默丢弃。✅
- **P2-fe-3**：`listen-captured`/preset/`remove_record_key`/`open_config_dir`/`quit_app` 等 ~10 处 invoke 无 catch，全进 `unhandledrejection` 弹"操作未完成"噪音（blur 清理路径也会弹）。✅
- **P2-fe-4**：8 个 `listen()` 的 unlisten 回调全部丢弃：webview 销毁即回收故当前无害，但 HMR/重挂会叠加监听。✅
- **P2-fe-5**：`renderMaps()` 每次 `replaceChildren` 全量重建 → 焦点丢失、动画打断、persist 后 select 重建；`applyPulse` 选中卡不 prepend 与 deck 排序约定不一致。✅
- **P2-fe-6**：`window.blur → closeRecord` 激进：Alt+Tab 看一眼即丢草稿（有后端兜底但反馈为零）。✅
- **P2-fe-7**：录制中 Tab 可聚焦按钮但 Enter/Space 一律被录成键 —— 键盘用户无法激活任何按钮（a11y 瑕疵）。✅
- **P2-fe-8**：`navigator.platform` 已废弃（main.ts:60）；preset 按钮缺 `type="button"`；`p.className` 双赋值死代码（278-279）；`main.ts:551-552` 冗余三元。✅
- **P2-fe-9**：`pending pulse` 在窗口隐藏时 rAF 暂停 → 旧脉冲状态重见天日后回放。✅
- **P2-fe-10**：`app_version`/版本展示 `v—` 占位符（index.html:104）观感差；"正在检查稳定版"对 beta 版名不副实。✅
- **P2-fe-11**：`xmbc-banner` 仅 boot 检测一次 + 无操作入口；mac 辅助功能授权失败时窗口内无"打开设置"按钮（入口只在托盘）。✅
- **P2-fe-12**：`closeRecord` 不重置 `confirming`：若 `take_record_keys` 挂起，下次录制 `record-ok` 永 disabled。✅
- **P2-fe-13**：录制遮罩空态立即被 `renderKeys([])` 覆盖成"空"（index.html:117 的"在听"是死文案）；`record-ok` 空缓冲可点却提示"录制已取消"（因果倒置）。✅

### 托盘与菜单
- **P2-menu-1**：`operation_status` 错误文案粘性不消：`refresh()` 不复位 `state.error`，修好后旧"操作失败"长挂；且错误统一塌缩"请在工作台检查"，具体原因（如"切换保持中不可快捷绑定"）被吞成通用文案 + 菜单项 disabled 无法复制。✅
- **P2-menu-2**：macOS 同一 MenuItem 双挂托盘+app 菜单（muda 0.15+ 应支持但本项目开发在 Windows，Mac 路径疑未实测）。❓
- **P2-menu-3**：`MenuState` 句柄不全（show/config/releases/quit 未保存）→ 未来 i18n 无法 set_text 只能整体重建；全部文案硬编码中文无 i18n 机制。✅
- **P2-menu-4**：`--quit` argv 无鉴权：任意本地进程可 `MouseInsight.exe --quit` 令应用退出（走 graceful shutdown 会释放键，无越权但值得记录）。✅
- **P2-menu-5**：`ExitRequested code.is_none()` 全拦 prevent_exit：macOS Dock Quit/系统注销/关机的清理机会被堵死，注入键在恰按住时可能残留。修法：code=None 路径也先 `engine::shutdown()`。✅
- **P2-menu-6**：`open_config_dir` macOS 用 `Command::status()` 同步等 `/usr/bin/open` 退出（LaunchServices 卡则主线程卡）；`explorer` 裸名 + spawn 成功≠窗口打开。✅

### 更新链路
- **P2-upd-1**：所有网络失败统一报"无法连接 GitHub"：404（仅 prerelease）/403 限流/超时/畸形响应不分。✅
- **P2-upd-2**：`html_url` 用响应值会与 `lib.rs:79-82` 白名单强耦合 — 现用硬编码 `RELEASES_URL/latest` 防钓鱼是对的，但白名单+前端两处需同步维护。✅
- **P2-upd-3**：`isNewer()` 同值算两遍（main.ts:1026-1027）小冗余；无启动自动检查（可能是有意）。✅

### UI 美学与一致性（styles.css/index.html 主会话直查 + copy-i18n/ux-flow）
- **P2-ui-1**：**styles.css 单文件双布局残迹**：1-167 行一整套侧边栏布局（`.app` grid/`.rail` flex column/`border-right`），169-222 行整体推翻重写为浮动胶囊导航 — `.rail`/`.rail-foot`/`.brand`/`@media 760px` 等大量规则二次覆盖成死代码；`.key-chip` 先设 mono 字体又在 210 行改回正文。可读性/维护性差，建议删旧布局块。✅
- **P2-ui-2**：游离色值：`.banner` 边框 `#b89956`、`.danger:hover` `#ad4e32`/`#b74f39` 不进色板、深色主题未适配；`.rail-section` 类 HTML 中不存在（死样式）。✅
- **P2-ui-3**：**术语大乱斗（30 项详见 copy-i18n 报告）**：同一静置态按钮四种叫法（"识别鼠标键/开始听/再听一颗/取消识别"）；恢复动作"继续映射"(窗口) vs "恢复映射"(托盘)；侧键"侧键 · 后" vs "前侧键" vs README 三种写法；手势"点按/短按/点触/滚动"四名；模块名"引擎/监听/钩子"三词；窗口名"控制面板/按键工作台/工作台/鼠标映射"四名。README 能力矩阵用旧术语与 UI 下拉项对不上（"跟随按住"在 UI 是死文案）。✅
- **P2-ui-4**：平台文案错位：mac 安全文案写"托盘"（macOS 叫菜单栏）；`cfg-path` tooltip 恒写"文件资源管理器"（mac 应是访达）；`prettyKeys` 只翻 8 个修饰 token，Space/Enter/F* 等全显英文；mac 上 Left Alt 不译 ⌥ Option、RWin 无符号。✅
- **P2-ui-5**：**触发模式术语与用户认知断层**：UI 下拉只有"点按/长按"和"切换保持"，"跟随按住/单次触发"由 dual 槽位只填一半推导 — `MODE_DESC.dual`（logic.ts:55）只解释了"只填长按=跟随"，**没写"只填短按=单次触发"**（README:118 有应用内没有）。✅
- **P2-ui-6**：叠卡设计一次只见一张卡，无"第 x/n 张"指示、无展开全部视图；已绑/未绑按钮仅靠"＋"前缀区分。✅
- **P2-ui-7**：长按阈值 400ms 硬编码不可见不可调（engine.rs:136）。✅
- **P2-ui-8**：pill 列固定 62px（styles.css:203）恰容 3 字，文案涨到 4 字即溢出 → 改 `minmax(62px,auto)`。✅
- **P2-ui-9**：小字号面：11-13px 大量用于 meta/pill/section label，低视力用户不友好；`:has()`/`overflow:clip`/`inert`/`100dvh`/`backdrop-filter` 在最低支持 WebView 上的兼容性需实测（`main.ts` 用 `navigator.platform` 判平台已废弃）。❓
- **P2-ui-10**：无 `prefers-reduced-motion` 处理（卡片拖拽/翻卡动画 420ms、`backdrop-filter blur(24px)` GPU 开销）— 流畅度在弱机/远程桌面上可能差。✅
- **P2-ui-11**：无 forced-colors/高对比模式适配；`.mask` 遮罩色 `#142b2566` 固定深绿在浅色主题下偏暗。✅
- **P2-ui-12**：aria 细节：`#pulse-name`/`#pulse-state` 无 aria-live（识别结果读屏不可见）；deck 按钮 aria-label"上一个按键"与可见"上一张"量词不一；录制 chips `data-key="LShift"` 显示"Shift"名实不符。✅
- **P2-ui-13**：识别期间滚轮一滚直接弹录制窗（engine.rs:2090-2097 把滚动归为 WheelUp/Down → main.ts:751-753 弹窗）— 最高频误触手势；建议弹窗标题写"检测到滚轮"或二次确认。✅
- **P2-ui-14**：对已有映射按键识别 → 直接覆盖式重录无预警；空映射卡无"不生效"提示无删除引导；滚轮 select 只有"单次触发"一项却仍可展开下拉。✅
- **P2-ui-15**："退出应用"无确认（设置区底部，风险低）；病句组：`设置${BUTTON_LABEL}的快捷键`→"设置侧键 · 后的快捷键"（双重间隔号链）、`README:157` 主语后多逗号、`index.html:58`"按下鼠标"→"按下鼠标按键"。✅

### 安全面（security 专项，威胁模型：本地工具，无远程内容无插件滥用）
- **P2-sec-1**：CSP 缺 `object-src 'none'`/`base-uri 'none'`/`form-action 'none'`（这三者**不回退** default-src）；`connect-src` 多放 `github.com`/`raw.`/`objects.githubusercontent.com`（updates.ts 只用 api.github.com）→ 可收紧。✅
- **P2-sec-2**：`cmd_tx` unbounded + command 无速率限制 → 本地自 DoS 面（devtools `for(;;)invoke`）；`cmd_tx` 可换 bounded 复用 edge 溢出→急停模式。✅
- **P2-sec-3**：config 读写不防符号链接：`config.json.bak` 为预置链接时 `fs::copy` 写穿任意文件（需同用户写权限，无权限边界跨越）。✅
- **P2-sec-4**：裸名 `cmd`/`explorer`/`open` PATH 解析（lib.rs:86,130,95）；mac native_menu 已用 `/usr/bin/open` 但 lib.rs:95 未统一 → 建议绝对路径。✅
- **P2-sec-5**：错误/eprintln 泄露绝对路径（含 `C:\Users\<username>`）；release 无控制台实际泄露面小，但 UI 显示 config 路径在录屏/远控时可见。✅
- **P2-sec-6**：**Windows 产物无 Authenticode 签名** — `WH_MOUSE_LL`+`WH_KEYBOARD_LL` 全局钩子 + `SendInput` + 录制期吞键正是 AV/EDR 的 keylogger 画像 → 未签名 + 便携 exe 旁落 `data/` 进一步降信誉，误报/拦截风险被放大（SmartScreen 首启必拦）。mac 侧已有 Developer ID+公证+hardenedRuntime 完整链。建议 Windows 接代码签名 + 发布页挂 SHA256SUMS（package-release.ps1:134-138 已生成）。✅
- **P2-sec-7**：`open_url` 的 `cmd /C start` 安全性完全靠两个精确字符串白名单（当前正确不可绕）；若未来放宽为 starts_with 则 `&`/`|` 经 cmd 二次解析立即命令注入 — 建议加注释警告或换 opener 插件。✅
- **P2-sec-8**：IPC 载荷无大小上限（`save_mappings` 反序列化先于校验，超长字符串先占内存）。✅

### 脚本/CI/发布
- **P2-scr-1**：~~latest.json URL 404~~ 已实测修正——远端 release 资产确为 `Mouse.Insight.exe` 点号命名（上传时改过名），URL 匹配不 404。**真正的问题**：本地产物 `Mouse Insight.exe`（空格）与发布资产 `Mouse.Insight.exe`（点号）名字不一致靠人工上传时改名维持，latest.json 的 URL 是在赌"每次都有人记得改名"——建议 package-release.ps1 直接把产物重命名为点号名再发布，消除人工环节。✅
- **P2-scr-2**：`source_dirty` 用 `git diff HEAD` 漏 untracked；git 调用无 `$LASTEXITCODE` 检查（非 git 目录报误导性错）；`CARGO_TARGET_DIR` 相对路径按仓库根解析而 cargo 按 workspace 根解析 → 误报 missing package。✅
- **P2-scr-3**：打包无条件重建 3 个快捷方式（用户删过的桌面图标被复活）；`--quit` 发向不支持该参数的旧版本时 helper 成孤儿进程；`config.json.*` 冲突保护只覆盖本体不覆盖 `.bak`/`.corrupted.*`；最终 Copy-Item 非原子。✅
- **P2-scr-4**：**CI 完全无 Rust 缓存**（ci.yml + macos-release.yml）：`cargo test`+`tauri build`（lto+cgu=1）全量冷编 → windows job 大概率逼近 45min timeout。加 `Swatinem/rust-cache@v2` 一行解决大头。✅
- **P2-scr-5**：`cargo test`/`cargo build` 未加 `--locked`（依赖可漂移）；CI 不跑 `check-release-version.mjs`（版本漂移只在打 tag 才发现）；action 全按 tag 浮动未钉 SHA。✅
- **P2-scr-6**：`macos-release.yml`：`--clobber` 可覆盖已 published release 产物（重跑 tag 不变换内容物）；清理步骤 `set +e`+`exit 0` 吞错；`security import -A` 过宽（-T 形同虚设）；`find-identity` 取第一个 Developer ID 多身份时可能选错；secrets 挂 job 级 env 对每个 step 可见。✅
- **P2-scr-7**：`verify-macos-bundle.sh` 签名身份只展示不断言（错团队 Developer ID 也能过）；无 `lipo -archs` universal 校验；`--deep` 已弃用（本 app 无嵌套 bundle 无实际影响）。✅
- **P2-scr-8**：`verify-ui.py` 8 处脆弱点：playwright 依赖无声明、`next()` 无 tauri page 时裸 StopIteration、7+ 处固定 `wait_for_timeout` 抖动即失败、`filter.slice(5)` 假定 `blur(` 前缀（reduced-motion 下 NaN）、L238 浮点严格相等 vs L235 容差不一致、原生 select 的 OS 级 popup Escape 怪癖、420ms settle 动画窗口依赖、中文文案断言耦合。且**不在 CI/无 npm script**，需隔离 portable review build + `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 开 CDP — 质量高但无法一键复现。✅
- **P2-scr-9**：`check-release-version.mjs:33` `npmLock.packages[""]` 无保护（lockfileVersion 1 裸 TypeError）；`:12` `^version` 顶格锚定对合法缩进 TOML 失配（fail-closed 可接受）。✅

### 文档
- **P2-doc-1**：README:33"约 10MB 内存"无测量证据且与自家验证文档"不含量化声明"自相矛盾 — 要么补实测要么软化。✅
- **P2-doc-2**：~~README:54"macOS 10.15+"无配置支撑~~（已核实 `tauri.macos.conf.json` 含 `minimumSystemVersion: "10.15"`，此子项不成立）；README:88 称托盘可给滚轮绑"长按"与代码+自身 :44/:118 矛盾；README:111"不可同时监听"夸大（LL 钩子链式共存只是互吞）；README 菜单名"打开控制面板/退出/开机自启动"与实际"打开按键工作台…/退出应用/开机自启"全不符；5 映射上限未写入 README。✅
- **P2-doc-3**：`docs/feedback-2026-09-08.md` 四条"待复测/待验证"未闭环：Windows 无法监听（修复已落地未确认）、mac 授权后重启无效、Mac 原生菜单、xbutton 方位 — 发布前需 Mac 实机验收。❓

### 构建/版本/仓库
- **P2-bld-1**：`crate-type = ["staticlib","cdylib","rlib"]` 中 `staticlib` 对纯 Tauri 应用多余（拖慢链接）；release 无显式 panic 策略（unwind 默认 + FFI 回调无 catch = abort 风险已列 P0-1）；`strip=true` 丢符号叠加无日志 = 崩溃完全不可诊断。✅
- **P2-bld-2**：`engines.node>=24` 对纯构建工具过苛（Node 22 LTS 用户无法 build）；CI node-version 已 24 一致，建议放宽或文档明示硬性要求。✅
- **P2-bld-3**：`windows = "0.58"` 偏旧且与 tauri 的 0.61.3 并存双版本（见 P2-eng-23b）；**`cargo audit` 已实测**：493 个依赖 **0 漏洞**，7 个 warning（6 个 unmaintained 传递依赖 + glib unsound，均非 Windows 构建面）；`npm audit` 因 npmmirror 未实现 audit 端点无法跑（registry 限制，非项目问题）。建议 CI 纳入 `cargo audit`。✅
- ~~P2-bld-4~~：**高 DPI 疑点已排除**（2026-09-12）—— manifest 确未声明 dpiAware，但 tao-0.35.3 `EventLoopAttributes.dpi_aware` 默认 true → 启动即 `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)`（event_loop.rs:189-190），运行时已 DPI 感知。非 bug。✅已排除
- **P2-bld-5**：版本一致性六处全部对齐 `0.2.0-beta.6`（package.json/tauri.conf.json/Cargo.toml/package-lock/Cargo.lock/tag v0.2.0-beta.6）✅；`identifier com.linc.mouseinsight` 一致；但 `mainBinaryName:"Mouse Insight"`（空格）与 latest.json 点号、plist `mouse_insight`（下划线）三种形态并存 — 见 P1-19/P2-scr-1。
- **P2-repo-1**：`release/` 本地堆积 `qa-beta5`/`qa-beta6`/`qa-beta6-final`/`review-20260908`/`publish-beta6` 过期目录（已 gitignore 故仅本地磁盘杂乱，发布前应清理）；`icons/android`/`icons/ios` 冗余移动平台资源。✅
- **P2-repo-2**：无 LICENSE/CHANGELOG 文件（开源项目发布要素）；`.gitignore` 配置正确（release/dist/target/gen/node_modules 全覆盖），工作树干净。✅

### 泄漏与生命周期
- **P2-leak-1**：`physical_down_set`/`swallowed_buttons` 无 reset（后者见 P1-12）；`HOOK_TID`/`RUN_LOOP_REF`/`TAP_PORT` 各竞态已列；telemetry 线程永驻 — 均已知。✅
- **P2-leak-2**：Windows 句柄管理正确（UnhookWindowsHookEx/UnregisterHotKey/CloseHandle 全覆盖，engine.rs:1807-1842,1320）；mac CFType RAII 正确 — **无泄漏问题面**。✅
- **P2-leak-3**：前端重复 `boot()`（关窗重开 = 全新 webview 上下文）天然规避监听叠加；当前安全但依赖"窗口真销毁"前提。✅

### 首启与平台
- **P2-first-1**：首启无引导页但动线尚可（hero→识别→录制→保存）；SmartScreen/AV 对未签名 exe 必拦（见 P2-sec-6）；mac 首启权限失败无窗内行动按钮（见 P1-47）。✅
- **P2-first-2**：卸载残留：安装版卸载不清 `%APPDATA%\MouseInsight`（NSIS 默认）；Run 键 `disable` 留 `StartupApproved` 孤儿值；mac plist 名 `mouse_insight` 非 bundle id 且 disable 不 `launchctl bootout`。✅
- **P2-first-3**：**未支持平台静默死分支**：`engine.rs:1629-1639` Linux 编译能过、`FakeInjector` 假装工作、`hook_status` 永卡 "starting"；`open_config_dir` Linux 无 else 返回 Ok 却只建了目录。建议 `compile_error!` 拒未支持 target。✅
- **P2-first-4**：NSIS `CheckIfAppIsRunning` 按进程名 `Mouse Insight.exe` 匹配 → 安装时会要求关闭/杀掉**其他目录下的便携实例**（双形态共存冲突）。✅
- **P2-first-5**：mac 只 staple 了 DMG，`.app` 本体无 stapled ticket → `verify-macos-bundle.sh:29` 的 `stapler validate "$APP"` 对未 staple app 行为待验证；拷出后首次启动 Gatekeeper 需联网查公证，离线可能被拦。❓
- **P2-first-6**：0 条映射时 `engine-status` 显示"映射已启用"（托盘菜单更诚实地显示"尚未配置映射"）；首次关窗进托盘无应用内提示，新用户可能误以为已退出。✅
- **P2-first-7**：F13-F24 前端 `codeToToken` 能产出但双端 `key_spec` 只认 F1-F12 → 录得到存不下（与 P1-3 同族，列此备忘）。✅

---

## P3 — 清理/坏味道（可批量处理）

- **死代码/死分支**：`DraftSession` 类（logic.ts:327-449，~120 行）+ `isButtonAllowedForMode` + `inferDefaultMode` + `initialKeys` + `Snapshot.listening` + `label` 字段双侧定义零读写 + `applyPulse`/`getStatusPill` 的 `mode==="hold"` 永假分支（normalizeMapping 已改写为 dual）+ `main.ts:278` className 双赋值 + `native_menu.rs:164` 重复 `use Submenu` + `engine.rs:1907-1911` 两相同分支 + `styles.css` 旧布局块（P2-ui-1）。
- **能力矩阵自相矛盾**：`isButtonAllowedForMode`（hold/click 也 true）vs `getAllowedModesForButton`（只 dual/toggle）— 误导性 API，两函数应共享矩阵。
- **normalize 双端发散**：TS `normalizeKeyChord`（trim+滤空+locale 排序）vs Rust `normalize_key_chord`（不 trim+字节序）— `["B","a"]` 两端排序结果不同；建议 TS 改码点比较、Rust 补 trim。`normalizeMapping` 的 `keys`/`tap_keys` 共享同一数组引用（埋雷）。
- **`dual+keys` 旧格式双端一致丢数据**：`hasSlots` 判定 + `mode!=Dual` 守卫共同导致旧配置 `dual`+`keys` 升级后绑定静默消失。
- **wheel normalize 丢 hold_keys**：仅有 hold 的滚轮映射被静默清空。
- **16 键/chord 上限前端无预防**（只走保存失败回滚）。
- **`isModifierKey` 对非 string 真值 TypeError**（当前调用路径不可达）。
- **dual 短按 pill 闪"按住中"**：`getStatusPill` 只看 `state.active` 不看 `state.mode`（Click 事件也 active）— 修 `state?.mode==="hold"` 条件。
- **autostart 字段冗余**：`config.autostart` 实为只写缓存（boot 每次被 `isEnabled()` 真值覆写）。
- **`menu_summary`/`menu_mappings` 与 `engine().expect` 风格不一致**（同文件其他地方用 `ENGINE.get()` 优雅判空）。
- **`EXTRA_INFO` 双份定义**（engine.rs:131 与 macos.rs:23 各写一遍 `0x4D49_484B`）。
- **`cfg!(test)` 双表回退隐蔽**：mac 测试构建下 engine 层 `key_spec`/`vk_to_token` 静默走 Windows 表 — mac CI 的 `cargo test` 实测的是 win 表，改表易漏测 mac。
- **快捷预设中英文标点/方括号/冒号全半角混用**约 10 处（copy-i18n #14）。
- **aria-label 与可见文案不一致**、chips 键名与录制后显示名不一致（Shift vs Left Shift）。
- **契约死面**（types-serde 专项）：`xmbc_running`/`config_dir` command 已注册但前端不直接 invoke（信息经 Snapshot 获取）；`Snapshot.listening`/`Pulse.t`/`SendReport.timestamp` emit 但前端不消费；`SendReport`/`RuntimeBindingState` 多 derive `Deserialize`（纯出站）；`SendReport.inserted` 双平台语义略异（Windows=实际插入数/mac=失败前处理数，前端仅展示无分支，可接受）；`runtime-binding-changed` 的 `mode` 是运行时态（Hold/Click/Toggle）永不发 Dual，未来若按 `payload.mode==="dual"` 判断会踩坑。

---

## 测试覆盖缺口（tests 专项 — 41 前端 + 52 Rust 用例现状）

- **P1**：Suite 4 九个用例测的是死代码 `DraftSession`（生产用裸 `currentDraft`），语义已发散（测试断言"纯修饰→hold"，生产产 dual）— 改坏真路径单测照绿。同理 `isButtonAllowedForMode`。
- **P1**：`set_mappings` 五条校验（>5/重复/主键/chord>16/非法 key）零单测（根因 `engine()` OnceLock 依赖，建议抽 `validate_mappings` 纯函数）。
- **P1**：`compile_mappings` 缺 hold-only/tap-only/toggle-keys 三用例；Dual 状态机缺 pending 期 pause/重复 down/tap 空槽三用例。
- **P1**：`send_mask`（VK_MASK_KEY）全测试 filter 掉零断言 — 该机制回归无测试会红。
- **P1**：钩子层"绝不拦左右键"的最终决策（`mouse_proc`/`handle_cgevent` 吞键判定）与 `classify()`（XBUTTON hi-word/wheel delta 方向）零覆盖 — 建议抽 `decide_swallow` 纯函数双端共用 + `classify` 表驱动测试。
- **P1**：前端真实编辑链路大面积无单测：mode select 切换 keys 迁移、`keysForSlot`、persist 失败回滚、removeKey 重建、button 冲突检测 — 建议纯函数下沉 logic.ts。
- **P2**：`state_commands_never_drop` 测的是 crossbeam 库本身（摆设）；`load_config` 损坏回退无测；`check-release-version.mjs` 无测；`npm test` 不含 `tsc --noEmit`（只 build 有）。
- **已验证为真的好测试面**：Key Ledger/引用计数/四触发模式/tap 上限/generation 过期/物理键抑制/急停 — Rust 侧断言真实非平凡；前端测试真断言真 import 同源逻辑；CI 双平台真跑。

---

## 已验证为正确/无问题的面（复核时不必重查）

- **四层左右键防护链**：hook 不吞（`is_primary`）→ 编译跳过 → `set_mappings` 拒绝 → UI 无入口 + listen-captured 显式拒绝 + sanitizeMappings 过滤 — 六层一致。
- **滚轮仅 click 强制链**：七层（前端 select 只渲染 click → normalize 强制 → sanitize → 后端 compile → quick_mapping 拒绝 → 托盘跳过 hold 子项）。
- **原子写**：tmp(pid+seq)+`sync_all`+`MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH)` 教科书正确；宽字符 PCWSTR/GetLastError 顺序正确。
- **cwd 无关性**：路径全基于 exe 目录/`%APPDATA%`/`HOME`，双击与命令行启动一致。
- **锁序**：全局仅 `cfg→SAVE_LOCK` 单向，无 ABBA 死锁；emit 全在 worker/telemetry 线程且不持锁 → 无 emit↔invoke 死锁环。
- **hook 回调全非阻塞**：try_send/unbounded send/原子/微秒级 parking_lot — 两平台核实，SendInput 正确隔离在 worker。
- **递归防护**：EXTRA_INFO 标记 + LLKHF/LLMHF_INJECTED 过滤 + CGEventSource CombinedSessionState — 注入事件不回环。
- **序列化契约**：Mapping/Snapshot/Pulse/SendReport/RuntimeBindingState 字段 snake_case 双端逐一匹配，TriggerMode rename_all lowercase 对齐，invoke 参数名核对一致 — **无 rename/camelCase 不匹配**。
- **急停三路兜底**：Pause/ScrollLock 热键（受 P0-4 占用风险）、edge 溢出自动急停（fails-open 正确）、窗口失焦/关闭 disarm — 主链完整。
- **macOS tap 生命周期**：TapDisabled re-enable 正确、runloop source 管理正确、Keep/Drop 语义+swallow 位图配对自洽。
- **退出路径**：quit→spawn_blocking→ResetState(Shutdown)→reset_all 释放→recv_timeout(800ms)→exit(0) — 正常路径完整。
- **更新安全**：全 HTTPS+8s abort+semver 正则+硬编码 URL 防钓鱼+Rust 侧二次白名单+textContent 无 XSS。
- **无遥测无第三方请求**（全仓仅 1 处 fetch）；physical_down_set 仅内存不落盘 — 隐私承诺兑现。
- **CI 真实**：双平台 cargo test+前端测试+打包+lipo 断言+签名公证链真实有效。
- **类型契约**：19 个 command 参数名与 invoke key 全等；8 个事件 emit↔listen payload 逐一匹配；字段 snake_case/Option↔optional/enum 值域全对齐 — types-serde 专项逐字段核实，无 P0 反序列化崩面。
- **已知问题闭环**：docs 三份文档逐条核对——宣称修复的条目全部在代码找到对应改动+回归测试，无"宣称已修实未修"项；4 条文档自述"待实机验证"（Windows 监听故障/mac 授权流/mac 菜单/物理侧键方位）仍为开口项。
- **依赖安全**：`cargo audit` 实测 493 依赖 0 漏洞（7 warning 均为传递依赖 unmaintained/unsound，非 Windows 构建面）。
- **远端发布**：`gh release view v0.2.0-beta.6` 实证存在、非 draft、Latest、资产齐备（exe+dmg+SHA256SUMS）。
- **构建与测试**：`npm run build` 干净；前端 41/41、Rust 52/52 全绿（本机实测）。

---

## 必须真机验证清单（静态无法定论的高价值项）

**Windows**：① SendInput Alt/Win mask 时序（P1-22，开始菜单抢焦）② UAC/锁屏/安全桌面切走时的注入键残留（P0-8）③ RegisterHotKey 被占用的实际概率（P0-4）④ LL hook `LowLevelHooksTimeout` 摘除实测（P1-11）⑤ wheel delta 归一化（P2-eng-1）⑥ SendInput 部分插入实测（P1-21）⑦ `wScan` 0xE0 前缀（P2-eng-23）⑧ 托盘 accelerator 是否派发（P1-34）⑨ `is_physical_down` 启动前按键漏检（P1-23a）。

**macOS**：① `CGEventSourceKeyState(HIDSystemState)` 是否被 HID-posted 合成事件污染（P2-eng-12，若污染=P0 级卡键）② 触控板连续滚动风暴量级（P1-4）③ `CGEventSource::new` 失败路径（P0-1）④ 授权后事件点不重建的已知怪癖（P2-eng 相关）⑤ `CFRunLoopStop` 是否需 `WakeUp`（P2-eng-13）⑥ Dock Quit/注销时 `ExitRequested` 序列（P2-menu-5）⑦ 菜单栏/Dock 图标 activation policy（autostart #9）⑧ 同一 MenuItem 双挂 muda 行为（P2-menu-2）⑨ 单反斜杠/小键盘回车实测（P1-2/P1-3）。

**跨平台**：① 单实例互斥会话作用域（runas/RDS 绕行）② APFS rename 目录 fsync 必要性 ③ WebView 对 `:has()`/`overflow:clip`/`inert`/`100dvh` 的最低版本支持。

---

## 修复优先级建议（给下游 Agent）

1. **先修 P0-1/P0-2/P0-9**（panic 兜底 + reset 错误上报 + 日志系统）— 三者是"出了问题能知道"的基础，其余修复都依赖可观测性。
2. **P0-8 物理状态 watchdog + P1-11 钩子看门狗** — 一个"周期 tick + GetAsyncKeyState/CGEventSourceButtonState 对账"同时消解：丢 up 卡键（拔线/睡眠/锁屏/UAC）、映射永久失效、swallowed_buttons 残留、睡眠 Instant 漂移，是本次审计**性价比最高的单点修复**。
3. **P0-4/P1-10**（急停热键注册结果 + hook_status 推送）— "用着用着不生效"是该类工具最高频报障。
4. **P0-6/P0-7**（锁内 IO 拆分 + invoke 超时）— 一次磁盘卡顿全链冻结是隐藏最深的卡死面。
5. **macOS 键表补齐 + 反斜杠笔误 + 授权重试**（P1-1/2/3/4/27/47）— 一次性把 mac 能力补到与 Windows 齐平。
6. **前端竞态族**（P1-5/6/7/13/14/29/38）— 统一引入 generation/epoch + listen 前置注册 + CAS save。
7. **UX 硬伤**（P1-9/15/16/24/25/26/46 + 术语统一 P2-ui-3/5）— 用户每天会踩的面。
8. **配置容错**（P1-17/18/43 + P2-cfg 系列）— BOM/逐条 salvage/便携形态固化都是小改动大收益。
9. **安装/卸载卫生**（P1-42 自启值名对齐、P1-44 WebView2 缺失提示、P2-first-4）+ **CI/发布**（P2-scr-4 rust-cache、--locked、产物命名统一）+ **Windows 签名**（P2-sec-6）。
10. **死代码清扫**（DraftSession/旧 CSS 布局块/死分支/契约死面）+ **测试补缺口**（set_mappings 校验/compile 模式/decide_swallow/send_mask）。

---

*报告生成：基于 43 项独立审计（34 子代理 + 9 主会话直查）+ 本机实测（build/test/cargo audit/gh release），全部结论以代码行号可复核为准；🟡/❓ 项请勿当作已确认 bug，按上表真机验证后定级。*
