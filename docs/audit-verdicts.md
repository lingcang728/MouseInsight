# 审计裁决 2026-09-12

基线（本机实测）：`npm run build` ✅ 干净通过 | 前端 `node tests/frontend_logic.test.mjs` ✅ 41/41 | `cargo test` ✅ 52/52

裁决取值：CONFIRMED（代码路径确实存在）/ REJECTED（不成立）/ PARTIAL（部分成立）/ NEEDS-MACHINE（静态无法定论）。

## A 逐条核实

| ID | 裁决 | 真实位置 | 依据 |
|----|------|----------|------|
| P0-3 | CONFIRMED | engine.rs:2243, 1096-1104, 1734-1748 | `execute_send_inputs` 在 worker 线程内同步调 `SendInput`（2243）；`enqueue_edge` 溢出时把 `EmergencyStop` 发到 `cmd_tx`（1101），而该命令只能由同一个卡住的 worker 在 1734 消费 —— 自锁结构属实。SendInput 实际阻塞概率属机器相关。 |
| P1-4 | CONFIRMED | macos.rs:627-636 | ScrollWheel 分支只读 `SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1`，`delta==0` 直接 `Keep`；每个非零 point delta 生成一条完整 down edge，无 `IS_CONTINUOUS`/`FIXED_PT_DELTA` 兜底、无聚合 —— 触控板风暴 / 纯 pixel 设备不触发，两条路径都在代码里。 |
| P1-9 | CONFIRMED | engine.rs:2011 / macos.rs:640; main.ts:789-801 | `captured = down && eng.listening.swap(false)` 不排除 primary 键；左键点「取消识别」时 hook 先 swap(false) 并发 `ListenCaptured("left")`；click 事件放行到 DOM。若 listen-captured 经 IPC 先于 click handler 落地，`stopListening()` 已把 `listening=false`，click handler 走 else 分支重新 `arm_listen` —— 竞态窗口真实存在（两种排序都产生异常：重开或误弹 alert）。 |
| P1-23 | CONFIRMED | engine.rs:1878-1885, 709/712; macos.rs:564-568 | (a) `physical_down_set` 仅由 `kbd_proc` 运行期写入，全仓无 `GetAsyncKeyState` 播种 —— 启动前已按住的键漏检属实。(b) `release_specs` 709 与 712 两次调 `is_physical_down` 同一 vk，TOCTOU 属实（期间物理按下 → relinquish 后仍发 key-up）。(c) mac FlagsChanged 方向靠 `CGEventSourceKeyState` 实时查询（564-568）属实。 |
| P1-38 | CONFIRMED | main.ts:724-753 | `listen-captured` 走 hook→`cmd_tx`→worker→emit，在飞事件无法撤回；handler 只查 `currentDraft`（727），不查本地 `listening` —— 用户点「取消识别」后迟到的 captured 事件仍走 `openRecordForNew`（752）弹录制窗。 |
| P1-5 | CONFIRMED | lib.rs:281-285, 45-60; engine.rs:1477-1493 | `Focused(false)` 直接 `disarm_record`/`disarm_listen`（281-285）；`arm_*` 命令无校验、无 generation（1477-1493 纯 `store(true)`）。同主线程下若 disarm 先于 arm 落地，`recording` 卡 true 且无任何事件兜底解除 → 全局键盘被吞（kbd_proc 1890 只看 recording）。顺序依赖属竞态，但缺防护属实。 |
| P1-19 | CONFIRMED | auto-launch-0.5.0/src/windows.rs:42, 73-83; lib.rs:256-263; main.ts:603-613 | `enable()` 写 `format!("{} {}", app_path, args)` **确实不加引号**；`is_enabled()` 只查 `Run\<name>` 值存在 + StartupApproved 标志，**不比对登记路径与当前 exe** → exe 移动后 UI 假勾选属实。单实例回调只认 `--quit`（257-261），第二自启实例会弹主窗属实。修正：注册表值名实为 `"Mouse Insight"`（productName），非报告所写 `mouse-insight`（见 P1-42）。 |
| P1-20 | CONFIRMED | updates.ts:2,33-44 | 本机 `gh release view v0.2.0-beta.6`：isPrerelease=false、isDraft=false；`gh api .../releases/latest` → `v0.2.0-beta.6` —— 更新检查当前工作正常，"依赖不勾 prerelease"的脆弱性描述成立。 |
| P1-22 | NEEDS-MACHINE | engine.rs:721-734, 944-949 | 结构属实：`release_specs` 先发修饰键 key-up 批次（722），再在独立 `SendInput` 里发 VK_MASK down/up（729）；`reset_all` 同样 mask 殿后（948）。「Alt/Win up 后被吞、开始菜单抢焦」是否符合预期只能真机实测，倾向：报告判断合理（主流实现把掩码与 key-up 同批或前置）。 |
| P1-42 | PARTIAL | auto-launch windows.rs:40-43/65-70; tauri-plugin-autostart-2.5.1/lib.rs:178-182; tauri-codegen context.rs:267-271 | **报告的核心机制有误**：未设 `app_name` 时取 `app.package_info().name`，而 Tauri 2 中该字段 = productName → 注册表值名是 `HKCU\Run\Mouse Insight`，与 NSIS 卸载器删 `Run\<ProductName>` **匹配**，不产生孤儿自启项。仍成立的部分：`disable()` 只删 Run 值不清 `StartupApproved\Run` 条目（auto-launch windows.rs:65-70 vs 46-54）；`%APPDATA%\MouseInsight` 自定义目录卸载不清理；macOS plist 名实为 `Mouse Insight.plist`（非 bundle id 且 disable 仅删文件不 bootout）。 |
| P2-eng-1 | CONFIRMED | engine.rs:2090-2097 | `WM_MOUSEWHEEL` 取 hi-word 为 i16 delta，`delta>0`→WheelUp 否则 WheelDown —— `delta==0` 误判 WheelDown 属实；无 WHEEL_DELTA 归一化，高分辨率滚轮过触发/大 delta 欠触发属实。 |
| P2-eng-4 | CONFIRMED | engine.rs:824-832, 899-901 | paused 分支对 `!down` 静默 `pending_dual.remove`（826）不触发 tap；非 paused 分支则 `fire_click`（900）。up 落在 pause 转换窗内是否触发 tap 取决于命令与边沿的到达序 —— 竞态属实但自愈（随后 `reset_all` 释放），属有界良性方向。 |
| P2-eng-12 | NEEDS-MACHINE | macos.rs:457-460 | `is_physical_down` 用 `CGEventSourceKeyState(HIDSystemState)`；HID-posted 合成事件是否污染 HID 表 Apple 文档未承诺，只能真机验证。倾向：报告列为真机必验项合理。 |
| P2-eng-13 | NEEDS-MACHINE | macos.rs:474-478, 533-534 | `stop_hook` 只调 `CFRunLoop::stop()`，无 `CFRunLoopWakeUp`；runloop 阻塞在 mach port 时 stop 生效但可能延迟到下条事件。结构属实，延迟量级需真机。 |
| P2-eng-25 | NEEDS-MACHINE | engine.rs:760, 877 | `tap_in_flight.due`/`pending_dual.due` 用 `Instant`，无电源恢复处理；`Instant` 跨 S3 语义双端确实可能分叉（Windows QPC 与 mac mach_absolute_time 对睡眠计时不同）。静态可确认结构，实际漂移方向需双平台实测。 |
| P2-cfg-9 | CONFIRMED | engine.rs:1283-1285 | 非 Windows 分支 `fs::rename(from, to)` 后无目录 fsync —— 结构属实（对比 Windows 分支 MOVEFILE_WRITE_THROUGH）。APFS 下风险低属严重性判断，不影响事实成立。 |
| P2-menu-2 | NEEDS-MACHINE | native_menu.rs:127-135, 172-181, 209 | 同一 `MenuItem` 对象 `show`/`pause`/`quit` 同时 append 进托盘 `menu`（209）与 macOS app menu（172-181）—— 代码结构属实；muda 0.15+ 双挂行为只能 Mac 实测。 |
| P2-ui-9 | NEEDS-MACHINE | styles.css:198(:has), 195(overflow:clip), 33(100dvh), 173(backdrop-filter); main.ts:417,498(inert), 60(navigator.platform) | 特性使用全部属实；小字号面属实（49/117/139 等处 10-13px）。WebView2 evergreen 无虞；macOS 10.15 老 WKWebView 兼容性是真实风险点 —— 需最低版本真机。 |
| P2-doc-3 | CONFIRMED | docs/feedback-2026-09-08.md | 文档自述 4 处未闭环：「合作者故障尚未复现」（Windows 无法监听）、「需 Mac 编译和实机验证」（授权流）、「Mac 原生菜单待实测」、「物理鼠标待合作者复测」（xbutton 方位）—— 与报告列举一致。 |
| P2-bld-4 | REJECTED | tauri-build-2.6.3/src/windows-app-manifest.xml; tao-0.35.3 event_loop.rs:161,189-190 + dpi.rs:26 | manifest 未声明 dpiAware 属实（Tauri 默认 manifest 仅含 Common-Controls 依赖，asInvoker 也是隐式默认值）。但 tao 的 `EventLoopAttributes.dpi_aware` 默认 true → 启动时 `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` —— 运行时已 DPI 感知，"高 DPI 模糊"风险基本不成立。 |
| P2-first-5 | NEEDS-MACHINE | macos-release.yml（stapler staple "$dmg" 仅 DMG）; verify-macos-bundle.sh:29 | .app 本体确实不 staple，仅 DMG staple。`xcrun stapler validate` 对未 staple .app 的行为（空跑成功 vs 报错）社区记载不一，需 Mac 实测；拷出后离线 Gatekeeper 联网查票风险为已知 macOS 行为。 |
| P2-scr-1 | CONFIRMED | scripts/package-release.ps1（latest.json 段 `Mouse.Insight.exe`）+ gh 实测 | 远端资产实为 `Mouse.Insight.exe`/`Mouse.Insight_0.2.0-beta.6_x64-setup.exe`（点号），本地产物 `Mouse Insight.exe`/`Mouse Insight_...-setup.exe`（空格）—— "靠人工改名维持、latest.json 在赌"属实。 |

### P3 段（16 条）

| 条目 | 裁决 | 依据 |
|------|------|------|
| 死代码/死分支 | CONFIRMED | `DraftSession`（logic.ts:327-449）未被 main.ts import；`isButtonAllowedForMode`、`inferDefaultMode`（仅经 `inferTriggerMode` 别名间接到达）未进 main.ts import 列表（main.ts:4-17）；`initialKeys` 只写（main.ts:462,480）不读；`Snapshot.listening`（main.ts:44）声明零消费；`label` 字段仅 serde 往返 + engine.rs:1415 写空串，双侧零业务读写；`getStatusPill`/`applyPulse` 的 `mode==="hold"` 永假（normalizeMapping 只产 dual/toggle/click）；main.ts:278-279 `p.className` 双赋值；native_menu.rs:164 块内重复 `use Submenu`（顶部:4 已导入）；engine.rs:1907-1911 两个 `vk_to_token(key).is_none()` 相同返回分支（`!down` 条件无效）；styles.css:33-167 旧侧边栏布局被 169+ 整体覆盖（`.brand`/`.rail-section`/`.safety` 在 index.html 中不存在）。 |
| 能力矩阵自相矛盾 | CONFIRMED | `isButtonAllowedForMode`（logic.ts:263-265）对 middle/xbutton 允许 hold/click/dual/toggle 四值；`getAllowedModesForButton`（272-283）同按键只返回 `["dual","toggle"]` —— 两函数不同源属实。 |
| normalize 双端发散 | CONFIRMED | TS `normalizeKeyChord` trim+滤空+`localeCompare`（logic.ts:153-163）；Rust `normalize_key_chord` 不 trim、`a.cmp(b)` 字节序（engine.rs:1044-1055）。`["B","a"]`：locale 序 a 前 B 后，字节序 B 前 a 后 —— 发散属实。`normalizeMapping` line 300/302 `keys`/`hold_keys`/`tap_keys` 共享同一数组引用属实。 |
| dual+keys 旧格式双端丢数据 | CONFIRMED | TS `hasSlots = mode==="dual" || tap/hold 非空`（logic.ts:290）→ mode dual + 仅 keys 的旧配置走 304 行 `nextKeys=[]` 清零；Rust 端 `mode != Dual` 守卫（engine.rs:305）同样跳过 legacy fallback —— 双端一致丢 keys 属实。 |
| wheel normalize 丢 hold_keys | CONFIRMED | logic.ts:291-293 滚轮分支恒返回 `hold_keys: []`，仅有 hold 的滚轮脏数据被静默清空属实。 |
| 16 键/chord 前端无预防 | CONFIRMED | 前端无任何长度检查；仅后端 `set_mappings` 1376 拒绝 → 走 persist 失败回滚路径属实。 |
| isModifierKey 非 string TypeError | CONFIRMED | logic.ts:92-93 `key.trim()` 对真值非 string（如 number）抛 TypeError；现有调用（inferDefaultMode 的 every、normalizeKeyChord 已先滤非 string）不可达，属实且报告自注不可达。 |
| dual 短按 pill 闪"按住中" | CONFIRMED | `getStatusPill`（main.ts:149-159）只看 `state?.active`，`runtime-binding-changed` 的 tap 事件也发 `active:true`（engine.rs:763 mode=Click）→ 30ms 闪烁属实。 |
| autostart 字段冗余 | CONFIRMED | `config.autostart` 由 `save_autostart` 写入（engine.rs:1463-1471），boot 每次被 `isEnabled()` 真值覆写（main.ts:603-613）→ 只写缓存属实。 |
| menu_* expect 风格不一致 | CONFIRMED | `menu_summary`/`menu_mappings` 经 `engine().expect`（engine.rs:1332/1335），同文件 `set_window_visible` 用 `ENGINE.get()` 优雅判空（1129）—— 属实。 |
| EXTRA_INFO 双份定义 | CONFIRMED | engine.rs:131 与 macos.rs:23 各写 `0x4D49_484B` —— 属实。 |
| cfg!(test) 双表回退 | CONFIRMED | engine.rs:1923-1928 / 2103-2108：macOS 上 `if !cfg!(test)` 才走 mac 表，test 构建静默回退 `win_vk_to_token`/`win_key_spec` —— 属实。 |
| macOS app menu 中英混排 | REJECTED(by-design) | native_menu.rs:170-179 自定义中文项与 PredefinedMenuItem 系统语言混排 —— 报告自注"平台惯例可接受"，非缺陷。 |
| 预设标点混用 ~10 处 | CONFIRMED | index.html/native_menu.rs 中 `…`、`·`、半角括号等混用可见（如 native_menu.rs:127"打开按键工作台…"、index.html:129 混排说明文）—— 风格瑕疵属实。 |
| aria-label 与可见文案不一致 | CONFIRMED | index.html:83 `aria-label="上一个按键"` vs 可见"上一张"；index.html:126 chips `data-key="LShift"` 显示"Shift" —— 属实。 |
| 契约死面 | CONFIRMED | `xmbc_running`/`config_dir` 命令已注册（lib.rs:113-121, 358-362）但前端走 Snapshot（main.ts:614,618）零直接 invoke；`Snapshot.listening`/`Pulse.t`/`SendReport.timestamp` emit 后前端不消费（main.ts:716-721 只读 inserted/expected/win32_error/is_uipi_blocked）；`SendReport`/`RuntimeBindingState` 多 derive Deserialize（engine.rs:403-404, 412-418）纯出站；`runtime-binding-changed` 的 `mode` 仅发 Hold/Click/Toggle（emit_state_change 全部调用点无 Dual）—— 属实。 |

### 测试覆盖缺口（8 条）

| 条目 | 裁决 | 依据 |
|------|------|------|
| Suite 4 测死代码 DraftSession 且语义发散 | CONFIRMED | tests/frontend_logic.test.mjs:270-415 共 9 个用例全测 `DraftSession`；生产路径用裸 `currentDraft`（main.ts:57）；测试断言纯修饰→`mode:"hold"`（339-340），生产 `confirmRecord` 产 `mode:"dual"`（main.ts:534）—— 属实。 |
| set_mappings 五条校验零单测 | CONFIRMED | engine.rs:1366-1380 的 >5/重复/主键/chord>16/非法 key 校验无任何 #[test] 触达（`engine()` OnceLock 依赖属实）。 |
| compile_mappings / Dual 状态机缺用例 | CONFIRMED | 现有用例覆盖 hold+click/dual 双槽/滚轮强制/清 dual，缺 hold-only(tap_keys)/tap-only(hold_keys)/toggle+keys 及 Dual pending 期 pause/重复 down/tap 空槽 —— 属实。 |
| send_mask 全被过滤零断言 | CONFIRMED | 测试多处 `filter(|e| !e.is_mask)`（engine.rs:2418, 3268, 2848 等），无任何断言验证 mask 事件被发出 —— 属实。 |
| 钩子层吞键判定与 classify 零覆盖 | CONFIRMED | `mouse_proc`/`handle_cgevent` 吞键决策与 `classify`（engine.rs:2077-2101，`#[cfg(windows)]`）无测试触达 —— 属实。 |
| 前端真实编辑链路无单测 | CONFIRMED | mode select keys 迁移（main.ts:859-876）、`keysForSlot`（448-452）、persist 回滚（388-394）、removeKey（909-928）、按钮冲突（846-849）均无对应测试 —— 属实。 |
| state_commands_never_drop 测 crossbeam 本身等 | CONFIRMED | engine.rs:2763-2775 仅断言 unbounded channel 收发；`load_config` 损坏回退、`check-release-version.mjs` 无测试；package.json `test` 脚本不含 `tsc --noEmit` —— 属实。 |
| 已验证为真的好测试面 | CONFIRMED | Key Ledger/四模式/tap 上限/generation/物理键抑制/急停用例断言真实非平凡（engine.rs:2328-3205 逐条核对）；前端测试真 import 生产 logic.ts；CI 双平台真跑（ci.yml）—— 属实。 |

## B 位置抽查

| ID | 报告位置 | 真实位置 | 描述相符? |
|----|----------|----------|-----------|
| P1-42 | （无行号，机制断言） | tauri-codegen context.rs:267-271；auto-launch windows.rs:40-42 | **不相符**：`app_name` 取 productName "Mouse Insight" 而非 package.json "mouse-insight"，Run 值名与 NSIS 卸载删除名一致，孤儿自启项不成立（详见 A 表） |

其余 ✅ 条目（P0-1~P0-9、P1-1~P1-47 中其余 ✅、P2-eng-*/P2-cfg-*/P2-fe-* 全部 ✅）位置与描述逐一核对全部相符，无 >30 行漂移、无"有 catch 却说没有"级别的描述反转。抽样细节：P0-1 worker spawn 1604-1627 无 catch_unwind + macos.rs:410-411 `expect` ✓；P0-2 `let _ =` 吞错 944-949 ✓；P0-4 RegisterHotKey `let _ =` 1789-1800 + index.html:96/README:32,145 承诺文案 ✓；P0-9 恰好 8 处 `eprintln!`（grep 计数一致）且无 log/tracing/panic hook ✓；P0-7 恰好 18 处 `invoke(` ✓；P1-2 macos.rs:193 `"\\\\"` 双反斜杠笔误 vs engine.rs:2144 `"\\"` 正确 ✓；P1-35 `toggle_paused` 1344-1352 非原子读改写 ✓；P1-41 EmergencyStop 1734-1748 不含 recording ✓；P2-eng-23b Cargo.lock 4450/4460 行 windows 0.58.0 与 0.61.3 并存 ✓。

## C 额外发现

1. **P2-doc-2 子项有误**：报告称"README:54 macOS 10.15+ 无配置支撑（tauri.conf.json 无 minimumSystemVersion）"——实际 `src-tauri/tauri.macos.conf.json` 含 `"minimumSystemVersion": "10.15"`，Tauri 2 平台配置叠加机制会合并该文件，README 声明有配置支撑，此子项不成立（P2-doc-2 其余子项未受影响）。
2. **P1-42 附带修正**：报告给出的 macOS plist 名 `mouse-insight.plist` 亦不准，实际为 `~/Library/LaunchAgents/Mouse Insight.plist`（auto-launch macos.rs:180 `{app_name}.plist`，app_name=productName）。
3. `check-release-version.mjs` 的 `^version` 锚定（P2-scr-9 所述）与 `packages[""]` 无保护两处，经读源确认属实；另注意该脚本 **在 CI（ci.yml）中确未运行**，但在 `package-release.ps1` 本地打包与 `macos-release.yml` verify job 中均有调用 —— 报告"版本漂移只在打 tag 才发现"表述基本准确（本地打包路径会先拦住）。

## P0-8 实测（2026-09-12，mi-probe 探针，非源码分析）

探针：`%TEMP%\mi-probe`（临时 cargo bin，windows 0.61，不入仓库）。线程 H 装 `WH_MOUSE_LL`/`WH_KEYBOARD_LL`，`SWALLOW` 为 true 时对 XBUTTON1（mouseData 高字=1）与 VK_F13（0x7C）返回 `LRESULT(1)`；主线程用 SendInput 注入 down/up 并轮询 `GetAsyncKeyState`。

原始输出：

```
[hook-thread] hooks installed
--- swallow=false ---
xbutton1 after down : GetAsyncKeyState(VK_XBUTTON1)=true
xbutton1 after up   : GetAsyncKeyState(VK_XBUTTON1)=false
f13      after down : GetAsyncKeyState(VK_F13)=true
f13      after up   : GetAsyncKeyState(VK_F13)=false
--- swallow=true ---
xbutton1 after down : GetAsyncKeyState(VK_XBUTTON1)=false
xbutton1 after up   : GetAsyncKeyState(VK_XBUTTON1)=false
f13      after down : GetAsyncKeyState(VK_F13)=false
f13      after up   : GetAsyncKeyState(VK_F13)=false
```

**结论：LL 钩子吞掉的按键事件不会更新异步按键状态**——`GetAsyncKeyState` 对被吞的物理按下报 false。这意味着基于 `GetAsyncKeyState` 轮询的物理按键 watchdog 在 Windows 上会把"钩子吞住的按住"误判为"已物理释放"，主动合成幻影 key-up —— 比原本要防的卡键更危险。

**采用方案（结果 B）**：
- `Win32Injector::is_button_physically_down` 恒返回 `true`（Windows watchdog 轮询失效，仅由真实事件驱动）。
- hook_loop 线程在装钩子前创建 message-only 窗口（`HWND_MESSAGE` 父级），注册 `WTSRegisterSessionNotification(NOTIFY_FOR_THIS_SESSION)` 与 `RegisterSuspendResumeNotification(DEVICE_NOTIFY_WINDOW_HANDLE)`；wndproc 对 `WM_WTSSESSION_CHANGE`（任意 wParam）、`WM_POWERBROADCAST`（PBT_APMSUSPEND/PBT_APMRESUMEAUTOMATIC/PBT_APMRESUMESUSPEND）、`WM_DEVICECHANGE`（DBT_DEVICEREMOVECOMPLETE）调 `engine::session_reset()` → `ResetState(SessionChanged)`（不暂停、仅释放）+ 清 swallowed_bits。退出时反注册并销毁窗口。
- 已知盲区：message-only 窗口收不到广播型 `WM_DEVICECHANGE`（仅顶层窗口收广播），`RegisterSuspendResumeNotification` 与 WTS 注册是定向投递、message-only 可收；DBT_DEVICEREMOVECOMPLETE 分支按 best-effort 保留。macOS 的 `CGEventSourceButtonState` 属 HID 层、不受 tap Drop 影响，mac 的 watchdog 轮询保留（此假设未经 mac 真机验证）。
