import assert from "node:assert/strict";
import {
  isModifierKey,
  modifierWeight,
  normalizeKeyChord,
  inferDefaultMode,
  inferTriggerMode,
  isButtonAllowedForMode,
  getAllowedModesForButton,
  sanitizeMappings,
  codeToToken,
  normalizeMapping,
  keysForSlot,
  hasButtonConflict,
  applyButtonChange,
  applyModeChange,
  removeKeyFromSlot,
} from "../src/logic.ts";

// 轻量、高可读性的纯逻辑测试执行器
const testStats = {
  total: 0,
  passed: 0,
  failed: 0,
  suites: 0,
};

async function suite(name, fn) {
  testStats.suites++;
  console.log(`\n\x1b[1m\x1b[36m▶ [Suite] ${name}\x1b[0m`);
  await fn();
}

async function test(name, fn) {
  testStats.total++;
  const start = performance.now();
  try {
    await fn();
    const duration = (performance.now() - start).toFixed(2);
    testStats.passed++;
    console.log(`  \x1b[32m✔\x1b[0m \x1b[2m${name}\x1b[0m \x1b[90m(${duration}ms)\x1b[0m`);
  } catch (err) {
    testStats.failed++;
    console.error(`  \x1b[31m✖\x1b[0m \x1b[1m\x1b[31m${name}\x1b[0m`);
    console.error(`    \x1b[31m${err.message}\x1b[0m`);
    if (err.stack) {
      console.error(`    \x1b[90m${err.stack.split("\n").slice(1, 4).join("\n    ")}\x1b[0m`);
    }
  }
}

console.log("\x1b[1m\x1b[35m=== MouseInsight 前端核心逻辑与事务化录制自动化测试 ===\x1b[0m");

// ============================================================================
// 1. isModifierKey 与 normalizeKeyChord / 修饰键判断测试
// ============================================================================
await suite("1. isModifierKey 与 normalizeKeyChord 修饰键规则", async () => {
  await test("准确识别标准及细分 Control 键 (Ctrl, LControl, RControl, leftctrl, Control)", () => {
    assert.equal(isModifierKey("Ctrl"), true);
    assert.equal(isModifierKey("ctrl"), true);
    assert.equal(isModifierKey("Control"), true);
    assert.equal(isModifierKey("control"), true);
    assert.equal(isModifierKey("LControl"), true);
    assert.equal(isModifierKey("RControl"), true);
    assert.equal(isModifierKey("leftctrl"), true);
    assert.equal(isModifierKey("rightctrl"), true);
  });

  await test("准确识别标准及细分 Alt 键 (Alt, LAlt, RAlt, leftalt, rightalt)", () => {
    assert.equal(isModifierKey("Alt"), true);
    assert.equal(isModifierKey("alt"), true);
    assert.equal(isModifierKey("LAlt"), true);
    assert.equal(isModifierKey("RAlt"), true);
    assert.equal(isModifierKey("leftalt"), true);
    assert.equal(isModifierKey("rightalt"), true);
  });

  await test("准确识别标准及细分 Shift 键 (Shift, LShift, RShift, leftshift, rightshift)", () => {
    assert.equal(isModifierKey("Shift"), true);
    assert.equal(isModifierKey("shift"), true);
    assert.equal(isModifierKey("LShift"), true);
    assert.equal(isModifierKey("RShift"), true);
    assert.equal(isModifierKey("leftshift"), true);
    assert.equal(isModifierKey("rightshift"), true);
  });

  await test("准确识别 Win / Meta / Cmd 键 (Win, LWin, RWin, Meta, Cmd, Command)", () => {
    assert.equal(isModifierKey("Win"), true);
    assert.equal(isModifierKey("win"), true);
    assert.equal(isModifierKey("LWin"), true);
    assert.equal(isModifierKey("RWin"), true);
    assert.equal(isModifierKey("leftwin"), true);
    assert.equal(isModifierKey("rightwin"), true);
    assert.equal(isModifierKey("Meta"), true);
    assert.equal(isModifierKey("lmeta"), true);
    assert.equal(isModifierKey("rmeta"), true);
    assert.equal(isModifierKey("Cmd"), true);
    assert.equal(isModifierKey("Command"), true);
  });

  await test("准确识别非修饰键为 false (Enter, Space, F5, KeyA, Tab, Escape, Delete 等)", () => {
    assert.equal(isModifierKey("Enter"), false);
    assert.equal(isModifierKey("Space"), false);
    assert.equal(isModifierKey("F5"), false);
    assert.equal(isModifierKey("F1"), false);
    assert.equal(isModifierKey("F12"), false);
    assert.equal(isModifierKey("KeyA"), false);
    assert.equal(isModifierKey("A"), false);
    assert.equal(isModifierKey("Tab"), false);
    assert.equal(isModifierKey("Escape"), false);
    assert.equal(isModifierKey("Backspace"), false);
    assert.equal(isModifierKey("Delete"), false);
    assert.equal(isModifierKey("ArrowUp"), false);
    assert.equal(isModifierKey("ArrowDown"), false);
    assert.equal(isModifierKey("1"), false);
    assert.equal(isModifierKey("Window"), false);
    assert.equal(isModifierKey(""), false);
    assert.equal(isModifierKey("   "), false);
    assert.equal(isModifierKey(null), false);
    assert.equal(isModifierKey(undefined), false);
  });

  await test("modifierWeight 严格保持 Ctrl(10) < Shift(20) < Alt(30) < Win(40) < 普通键(100)", () => {
    assert.equal(modifierWeight("LControl"), 10);
    assert.equal(modifierWeight("RControl"), 10);
    assert.equal(modifierWeight("LShift"), 20);
    assert.equal(modifierWeight("RShift"), 20);
    assert.equal(modifierWeight("LAlt"), 30);
    assert.equal(modifierWeight("RAlt"), 30);
    assert.equal(modifierWeight("LWin"), 40);
    assert.equal(modifierWeight("RWin"), 40);
    assert.equal(modifierWeight("Enter"), 100);
    assert.equal(modifierWeight("Space"), 100);
    assert.equal(modifierWeight("F5"), 100);
  });

  await test("normalizeKeyChord 自动去重、过滤空白并规范排序", () => {
    // 重复项去重
    assert.deepEqual(
      normalizeKeyChord(["LControl", "LControl", "LAlt"]),
      ["LControl", "LAlt"]
    );
    // 逆序输入按权重重排: Enter(100), LAlt(30), LControl(10) -> Ctrl, Alt, Enter
    assert.deepEqual(
      normalizeKeyChord(["Enter", "LAlt", "LControl"]),
      ["LControl", "LAlt", "Enter"]
    );
    // 经典 Typeless 双键: Alt + Ctrl -> Ctrl + Alt
    assert.deepEqual(
      normalizeKeyChord(["LAlt", "LControl"]),
      ["LControl", "LAlt"]
    );
    // 包含 Shift, Win, 普通键复合组合
    assert.deepEqual(
      normalizeKeyChord(["Space", "RWin", "LShift", "LControl", "LAlt"]),
      ["LControl", "LShift", "LAlt", "RWin", "Space"]
    );
    // 过滤空串与异常输入
    assert.deepEqual(
      normalizeKeyChord(["", "  ", "Enter", null, undefined]),
      ["Enter"]
    );
    assert.deepEqual(normalizeKeyChord([]), []);
  });
});

// ============================================================================
// 2. 默认模式推断规则 (inferDefaultMode)
// ============================================================================
await suite("2. 默认模式推断规则 inferDefaultMode", async () => {
  await test("纯修饰键组合 -> 必须推断为 'hold' (跟随按住)", () => {
    assert.equal(inferDefaultMode(["LControl", "LAlt"]), "hold");
    assert.equal(inferDefaultMode(["Shift"]), "hold");
    assert.equal(inferDefaultMode(["LControl"]), "hold");
    assert.equal(inferDefaultMode(["LShift", "RAlt"]), "hold");
    assert.equal(inferDefaultMode(["LControl", "LShift", "LAlt", "LWin"]), "hold");
  });

  await test("包含非修饰键组合 -> 必须推断为 'click' (单次触发)", () => {
    assert.equal(inferDefaultMode(["Enter"]), "click");
    assert.equal(inferDefaultMode(["LControl", "Enter"]), "click");
    assert.equal(inferDefaultMode(["Space"]), "click");
    assert.equal(inferDefaultMode(["F5"]), "click");
    assert.equal(inferDefaultMode(["LControl", "LAlt", "Delete"]), "click");
    assert.equal(inferDefaultMode(["LShift", "A"]), "click");
    assert.equal(inferDefaultMode(["Tab"]), "click");
    assert.equal(inferDefaultMode(["Escape"]), "click");
  });

  await test("空键列表 -> 必须默认推断为 'click'", () => {
    assert.equal(inferDefaultMode([]), "click");
    assert.equal(inferDefaultMode(null), "click");
    assert.equal(inferDefaultMode(undefined), "click");
  });

  await test("滚轮按键 (wheelup / wheeldown) 无论录制什么键，必须强制为 'click'", () => {
    // 即使输入的是纯修饰键，只要是滚轮，也必须是 click
    assert.equal(inferDefaultMode(["LControl", "LAlt"], "wheelup"), "click");
    assert.equal(inferDefaultMode(["Shift"], "wheeldown"), "click");
    assert.equal(inferDefaultMode(["LControl"], "wheelup"), "click");
    // 普通键更是 click
    assert.equal(inferDefaultMode(["ArrowUp"], "wheelup"), "click");
    assert.equal(inferDefaultMode(["ArrowDown"], "wheeldown"), "click");
    assert.equal(inferDefaultMode([], "wheelup"), "click");
    assert.equal(inferDefaultMode([], "wheeldown"), "click");
  });

  await test("推断模式绝不自动推荐 'toggle' (仅在 hold 与 click 间推断)", () => {
    const testCases = [
      ["LControl"],
      ["LControl", "LAlt"],
      ["Enter"],
      ["Space"],
      ["LControl", "C"],
      [],
    ];
    for (const tc of testCases) {
      const mode = inferDefaultMode(tc);
      assert.notEqual(mode, "toggle");
      assert.ok(mode === "hold" || mode === "click");
    }
  });

  await test("inferTriggerMode 兼容别名函数行为完全一致", () => {
    assert.equal(inferTriggerMode("middle", ["LControl", "LAlt"]), "hold");
    assert.equal(inferTriggerMode("xbutton1", ["Enter"]), "click");
    assert.equal(inferTriggerMode("wheelup", ["LControl", "LAlt"]), "click");
  });
});

// ============================================================================
// 3. 能力矩阵规则 (isButtonAllowedForMode)
// ============================================================================
await suite("3. 按钮能力矩阵规则 isButtonAllowedForMode", async () => {
  await test("left / right 严格不允许任何模式的映射 (仅硬件检测，绝不干预)", () => {
    for (const btn of ["left", "right"]) {
      assert.equal(isButtonAllowedForMode(btn, "hold"), false);
      assert.equal(isButtonAllowedForMode(btn, "click"), false);
      assert.equal(isButtonAllowedForMode(btn, "toggle"), false);
      assert.equal(isButtonAllowedForMode(btn, "custom"), false);
      assert.deepEqual(getAllowedModesForButton(btn), []);
    }
  });

  await test("wheelup / wheeldown 仅允许 'click'，严禁 'hold' 或 'toggle'", () => {
    for (const btn of ["wheelup", "wheeldown"]) {
      assert.equal(isButtonAllowedForMode(btn, "click"), true);
      assert.equal(isButtonAllowedForMode(btn, "hold"), false);
      assert.equal(isButtonAllowedForMode(btn, "toggle"), false);
      assert.deepEqual(getAllowedModesForButton(btn), ["click"]);
    }
  });

  await test("middle, xbutton1, xbutton2 允许 dual 与 toggle（由 getAllowedModesForButton 派生）", () => {
    for (const btn of ["middle", "xbutton1", "xbutton2"]) {
      assert.equal(isButtonAllowedForMode(btn, "dual"), true);
      assert.equal(isButtonAllowedForMode(btn, "toggle"), true);
      assert.equal(isButtonAllowedForMode(btn, "hold"), false);
      assert.equal(isButtonAllowedForMode(btn, "click"), false);
      assert.equal(isButtonAllowedForMode(btn, "invalid"), false);
      assert.deepEqual(getAllowedModesForButton(btn), ["dual", "toggle"]);
    }
  });

  await test("非法或未知按键名称返回 false", () => {
    assert.equal(isButtonAllowedForMode("button99", "click"), false);
    assert.equal(isButtonAllowedForMode("", "click"), false);
  });
});


// ============================================================================
// 5. 数据清洗与安全性校验 (sanitizeMappings)
// ============================================================================
await suite("5. 数据清洗与安全性校验 sanitizeMappings", async () => {
  await test("严格过滤 left 与 right 映射配置", () => {
    const raw = [
      { id: "1", button: "left", mode: "click", keys: ["Enter"] },
      { id: "2", button: "right", mode: "hold", keys: ["LControl"] },
      { id: "3", button: "middle", mode: "click", keys: ["Space"] },
    ];
    const cleaned = sanitizeMappings(raw);
    assert.equal(cleaned.length, 1);
    assert.equal(cleaned[0].button, "middle");
  });

  await test("同按钮重复映射仅保留首个有效配置 (按键独占)", () => {
    const raw = [
      { id: "1", button: "xbutton1", mode: "hold", keys: ["LControl"] },
      { id: "2", button: "xbutton1", mode: "click", keys: ["Enter"] },
    ];
    const cleaned = sanitizeMappings(raw);
    assert.equal(cleaned.length, 1);
    assert.equal(cleaned[0].id, "1");
    assert.deepEqual(cleaned[0].keys, ["LControl"]);
  });

  await test("滚轮映射如果为非 click 模式，自动纠正为 click", () => {
    const raw = [
      { id: "1", button: "wheelup", mode: "hold", keys: ["Up"] },
      { id: "2", button: "wheeldown", mode: "toggle", keys: ["Down"] },
    ];
    const cleaned = sanitizeMappings(raw);
    assert.equal(cleaned.length, 2);
    assert.equal(cleaned[0].mode, "click");
    assert.equal(cleaned[1].mode, "click");
  });
});

await suite("6. normalizeMapping 短按/长按字段", async () => {
  await test("旧 hold 配置迁移到 hold_keys", () => {
    const n = normalizeMapping({
      id: "1",
      button: "xbutton1",
      mode: "hold",
      keys: ["LControl", "LAlt"],
    });
    assert.equal(n.mode, "dual");
    assert.deepEqual(n.hold_keys, ["LControl", "LAlt"]);
    assert.deepEqual(n.tap_keys, []);
  });

  await test("旧 click 配置迁移到 tap_keys", () => {
    const n = normalizeMapping({
      id: "2",
      button: "xbutton2",
      mode: "click",
      keys: ["Enter"],
    });
    assert.equal(n.mode, "dual");
    assert.deepEqual(n.tap_keys, ["Enter"]);
    assert.deepEqual(n.hold_keys, []);
  });

  await test("同时有 tap_keys 与 hold_keys 时保持 dual", () => {
    const n = normalizeMapping({
      id: "3",
      button: "middle",
      mode: "dual",
      keys: [],
      tap_keys: ["LControl", "V"],
      hold_keys: ["LControl", "C"],
    });
    assert.equal(n.mode, "dual");
    assert.deepEqual(n.tap_keys, ["LControl", "V"]);
    assert.deepEqual(n.hold_keys, ["LControl", "C"]);
  });
});

await suite("7. codeToToken 键盘事件兜底映射", async () => {
  await test("左右修饰键映射到 L/R token", () => {
    assert.equal(codeToToken("ControlLeft"), "LControl");
    assert.equal(codeToToken("ControlRight"), "RControl");
    assert.equal(codeToToken("AltLeft"), "LAlt");
    assert.equal(codeToToken("AltRight"), "RAlt");
    assert.equal(codeToToken("ShiftLeft"), "LShift");
    assert.equal(codeToToken("MetaLeft"), "LWin");
  });

  await test("字母数字与功能键", () => {
    assert.equal(codeToToken("KeyA"), "A");
    assert.equal(codeToToken("Digit1"), "1");
    assert.equal(codeToToken("Enter"), "Enter");
    assert.equal(codeToToken("F5"), "F5");
    assert.equal(codeToToken("Space"), "Space");
  });

  await test("无法识别的 code 返回 null", () => {
    assert.equal(codeToToken(""), null);
    assert.equal(codeToToken("Unidentified"), null);
  });
});

// ============================================================================
await suite("8. 映射编辑回归", async () => {
  await test("重录右 Alt 后兼容 keys 不再保留旧左 Alt", () => {
    const mapping = normalizeMapping({ id: "alt", button: "xbutton1", mode: "dual", keys: ["LAlt"], tap_keys: [], hold_keys: ["RAlt"] });
    assert.deepEqual(mapping.keys, ["RAlt"]);
    assert.deepEqual(normalizeMapping(mapping), mapping);
  });
  await test("清空 dual 两个槽位与 keys 后不复活历史快捷键", () => {
    const mapping = normalizeMapping({ id: "empty", button: "middle", mode: "dual", keys: [], tap_keys: [], hold_keys: [] });
    assert.deepEqual(mapping.keys, []);
    assert.deepEqual(mapping.tap_keys, []);
    assert.deepEqual(mapping.hold_keys, []);
  });
  await test("旧后端序列化的空槽位仍能迁移 hold", () => {
    const mapping = normalizeMapping({ id: "legacy", button: "middle", mode: "hold", keys: ["RAlt"], tap_keys: [], hold_keys: [] });
    assert.deepEqual(mapping.hold_keys, ["RAlt"]);
  });
  await test("未知鼠标键过滤、键名去空白", () => {
    assert.deepEqual(sanitizeMappings([{ id: "bad", button: "unknown", mode: "click", keys: ["A"] }]), []);
    assert.deepEqual(normalizeKeyChord([" RAlt ", "RAlt", " "]), ["RAlt"]);
  });
});

await suite("9. 版本与网络回归", async () => {
  const { isNewer, latestRelease } = await import("../src/updates.ts");
  await test("稳定版高于同版本 beta、数字预发布标识按数值比较", () => {
    assert.equal(isNewer("v0.2.0", "0.2.0-beta.3"), true);
    assert.equal(isNewer("0.2.0-beta.10", "0.2.0-beta.9"), true);
    assert.equal(isNewer("0.2.0-beta.3", "0.2.0"), false);
    assert.equal(isNewer("0.2.0+build.2", "0.2.0+build.1"), false);
    assert.equal(isNewer("invalid", "0.2.0"), false);
  });
  await test("失败可重试，并发请求合并且只缓存成功结果", async () => {
    const original = globalThis.fetch;
    let requests = 0;
    globalThis.fetch = async () => {
      requests++;
      if (requests === 1) throw new Error("offline");
      return new Response(JSON.stringify({ tag_name: "v0.2.0", html_url: "https://untrusted.invalid" }));
    };
    try {
      await assert.rejects(latestRelease(), /offline/);
      const [a, b] = await Promise.all([latestRelease(), latestRelease()]);
      assert.deepEqual(a, b);
      assert.equal(a.url, "https://github.com/lingcang728/MouseInsight/releases/latest");
      await latestRelease();
      assert.equal(requests, 2);
    } finally { globalThis.fetch = original; }
  });
});

await suite("10. 别名归一与旧格式迁移", async () => {
  const { canonicalizeToken } = await import("../src/logic.ts");

  await test("canonicalizeToken 将常见别名归一到规范 token", () => {
    assert.equal(canonicalizeToken("cmd"), "LWin");
    assert.equal(canonicalizeToken("ctrl"), "LControl");
    assert.equal(canonicalizeToken("RightAlt"), "RAlt");
    assert.equal(canonicalizeToken("Enter"), "Enter");
  });

  await test("normalizeKeyChord 普通键按码点序排序", () => {
    assert.deepEqual(normalizeKeyChord(["B", "a"]), ["B", "a"]);
  });

  await test("旧 dual + keys 格式迁移到 tap_keys，不丢键", () => {
    const n = normalizeMapping({ id: "d1", button: "middle", mode: "dual", keys: ["LControl", "C"] });
    assert.deepEqual(n.tap_keys, ["LControl", "C"]);
  });

  await test("滚轮映射保留 hold_keys 数据（编译端忽略）", () => {
    const n = normalizeMapping({
      id: "w1", button: "wheelup", mode: "click",
      keys: [], tap_keys: ["A"], hold_keys: ["B"],
    });
    assert.deepEqual(n.hold_keys, ["B"]);
    assert.deepEqual(n.tap_keys, ["A"]);
  });

  await test("toggle 映射保留 tap_keys/hold_keys 数据", () => {
    const n = normalizeMapping({
      id: "t1", button: "xbutton1", mode: "toggle",
      keys: ["Enter"], tap_keys: ["A"], hold_keys: ["B"],
    });
    assert.equal(n.mode, "toggle");
    assert.deepEqual(n.tap_keys, ["A"]);
    assert.deepEqual(n.hold_keys, ["B"]);
  });

  await test("isButtonAllowedForMode 与能力矢量一致", () => {
    assert.equal(isButtonAllowedForMode("middle", "hold"), false);
    assert.equal(isButtonAllowedForMode("middle", "dual"), true);
  });
});

// ============================================================================
// 11. 映射编辑纯函数（applyModeChange / applyButtonChange / removeKeyFromSlot / keysForSlot）
// ============================================================================
await suite("11. 映射编辑纯函数", async () => {
  const dualMap = () => ({
    id: "m1", button: "xbutton1", mode: "dual",
    keys: ["LControl", "C"], tap_keys: ["LControl", "C"], hold_keys: ["LShift", "V"],
  });

  await test("applyModeChange dual→toggle 保留 tap/hold 且 keys 回填 tap", () => {
    const n = applyModeChange(dualMap(), "toggle");
    assert.equal(n.mode, "toggle");
    assert.deepEqual(n.keys, ["LControl", "C"]);
    assert.deepEqual(n.tap_keys, ["LControl", "C"]);
    assert.deepEqual(n.hold_keys, ["LShift", "V"]);
  });

  await test("applyModeChange toggle→dual 恢复 tap/hold 槽位", () => {
    const t = { id: "m2", button: "middle", mode: "toggle", keys: ["Enter"], tap_keys: ["A"], hold_keys: ["B"] };
    const n = applyModeChange(t, "dual");
    assert.equal(n.mode, "dual");
    assert.deepEqual(n.tap_keys, ["A"]);
    assert.deepEqual(n.hold_keys, ["B"]);
  });

  await test("applyModeChange →click 保留 hold_keys 数据", () => {
    const w = { id: "m3", button: "wheelup", mode: "dual", keys: [], tap_keys: ["A"], hold_keys: ["B"] };
    const n = applyModeChange(w, "click");
    assert.equal(n.mode, "click");
    assert.deepEqual(n.hold_keys, ["B"]);
  });

  await test("applyButtonChange 冲突返回 null，不改原映射", () => {
    const all = [dualMap(), { id: "m9", button: "middle", mode: "dual", keys: [], tap_keys: ["X"], hold_keys: [] }];
    assert.equal(applyButtonChange(all[0], "middle", all), null);
    const n = applyButtonChange(all[0], "xbutton2", all);
    assert.ok(n);
    assert.equal(n.button, "xbutton2");
  });

  await test("applyButtonChange 改到滚轮强制 click 且保留 hold_keys", () => {
    const n = applyButtonChange(dualMap(), "wheeldown", [dualMap()]);
    assert.ok(n);
    assert.equal(n.button, "wheeldown");
    assert.equal(n.mode, "click");
    assert.deepEqual(n.hold_keys, ["LShift", "V"]);
    assert.deepEqual(n.tap_keys, ["LControl", "C"]);
  });

  await test("removeKeyFromSlot 删 tap 键后 keys 回填", () => {
    const n = removeKeyFromSlot(dualMap(), "tap", "C");
    assert.deepEqual(n.tap_keys, ["LControl"]);
    assert.deepEqual(n.keys, ["LControl"]);
    assert.deepEqual(n.hold_keys, ["LShift", "V"]);
  });

  await test("removeKeyFromSlot toggle 槽只删 keys", () => {
    const t = { id: "m4", button: "middle", mode: "toggle", keys: ["LControl", "Enter"], tap_keys: ["A"], hold_keys: [] };
    const n = removeKeyFromSlot(t, "toggle", "Enter");
    assert.deepEqual(n.keys, ["LControl"]);
    assert.deepEqual(n.tap_keys, ["A"]);
  });

  await test("keysForSlot 三槽读取", () => {
    const m = dualMap();
    assert.deepEqual(keysForSlot(m, "tap"), ["LControl", "C"]);
    assert.deepEqual(keysForSlot(m, "hold"), ["LShift", "V"]);
    const t = { id: "m5", button: "middle", mode: "toggle", keys: ["Enter"], tap_keys: [], hold_keys: [] };
    assert.deepEqual(keysForSlot(t, "toggle"), ["Enter"]);
  });

  await test("keysForSlot tap 对历史 click 映射回退 keys", () => {
    const m = { id: "m6", button: "xbutton1", mode: "click", keys: ["Enter"] };
    assert.deepEqual(keysForSlot(m, "tap"), ["Enter"]);
  });

  await test("hasButtonConflict 排除自身", () => {
    const all = [dualMap(), { id: "m8", button: "middle", mode: "dual", keys: [], tap_keys: ["X"], hold_keys: [] }];
    assert.equal(hasButtonConflict(all, "m1", "xbutton1"), false);
    assert.equal(hasButtonConflict(all, "m1", "middle"), true);
  });
});

// 测试执行汇总汇报
// ============================================================================
console.log("\n------------------------------------------------------------");
console.log(`\x1b[1m测试套件汇总:\x1b[0m ${testStats.suites} 个套件, 共 ${testStats.total} 个测试用例`);
if (testStats.failed === 0) {
  console.log(`\x1b[32m✔ 全部通过: ${testStats.passed} passed, 0 failed\x1b[0m`);
  console.log("------------------------------------------------------------\n");
  process.exit(0);
} else {
  console.error(`\x1b[31m✖ 测试失败: ${testStats.failed} failed, ${testStats.passed} passed\x1b[0m`);
  console.log("------------------------------------------------------------\n");
  process.exit(1);
}
