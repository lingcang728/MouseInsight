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
  DraftSession,
  codeToToken,
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

  await test("middle, xbutton1, xbutton2 允许完整的 hold, click, toggle 三种模式", () => {
    for (const btn of ["middle", "xbutton1", "xbutton2"]) {
      assert.equal(isButtonAllowedForMode(btn, "hold"), true);
      assert.equal(isButtonAllowedForMode(btn, "click"), true);
      assert.equal(isButtonAllowedForMode(btn, "toggle"), true);
      assert.equal(isButtonAllowedForMode(btn, "invalid"), false);
      assert.deepEqual(getAllowedModesForButton(btn), ["hold", "click", "toggle"]);
    }
  });

  await test("非法或未知按键名称返回 false", () => {
    assert.equal(isButtonAllowedForMode("button99", "click"), false);
    assert.equal(isButtonAllowedForMode("", "click"), false);
  });
});

// ============================================================================
// 4. 事务化录制状态机 (DraftSession 逻辑)
// ============================================================================
await suite("4. 事务化录制状态机 DraftSession", async () => {
  await test("创建 draft 时绝不污染现有 mappings", () => {
    const initialMappings = [
      { id: "m1", button: "xbutton1", mode: "hold", keys: ["LControl", "LAlt"] },
    ];
    const session = new DraftSession(initialMappings);

    // 开启为 xbutton2 录制新按键
    const draft = session.startNewDraft("xbutton2");

    assert.equal(draft.button, "xbutton2");
    assert.equal(draft.isNew, true);
    assert.deepEqual(draft.initialKeys, []);

    // 验证当前 mappings 依然只有一条，长度为 1，完全不受污染！
    const currentMappings = session.getMappings();
    assert.equal(currentMappings.length, 1);
    assert.equal(currentMappings[0].id, "m1");
  });

  await test("严禁对 left / right 开启录制草稿", () => {
    const session = new DraftSession([]);
    assert.throws(() => {
      session.startNewDraft("left");
    }, /左\/右键/);
    assert.throws(() => {
      session.startNewDraft("right");
    }, /左\/右键/);
    assert.equal(session.getCurrentDraft(), null);
  });

  await test("新建录制中取消 (Cancel / Esc / Blur) 直接抛弃 draft，mappings 保持原状，不遗留空 mapping", () => {
    const session = new DraftSession([]);

    session.startNewDraft("middle");
    assert.ok(session.getCurrentDraft() !== null);

    // 取消录制（用户按 Esc / 点击取消 / 窗口失焦 Blur）
    const cancelled = session.cancelDraft();
    assert.equal(cancelled, true);

    // draft 清空，mappings 为空，没有任何空 mapping 遗留！
    assert.equal(session.getCurrentDraft(), null);
    assert.equal(session.getMappings().length, 0);
  });

  await test("新建录制确认 (Confirm)：空键时取消并不产生空 mapping", () => {
    const session = new DraftSession([]);
    session.startNewDraft("xbutton1");

    const result = session.confirmDraft([]);
    assert.equal(result.success, false);
    assert.equal(result.reason, "EMPTY_KEYS");

    assert.equal(session.getCurrentDraft(), null);
    assert.equal(session.getMappings().length, 0);
  });

  await test("新建录制确认 (Confirm)：纯修饰键自动推断为 'hold' 并原子写入 mappings", () => {
    let mockIdCounter = 1;
    const session = new DraftSession([], () => `id-${mockIdCounter++}`);

    session.startNewDraft("xbutton1");
    const result = session.confirmDraft(["LControl", "LAlt"]);

    assert.equal(result.success, true);
    assert.ok(result.mapping);
    assert.equal(result.mapping.id, "id-1");
    assert.equal(result.mapping.button, "xbutton1");
    assert.equal(result.mapping.mode, "hold"); // 纯修饰键 -> hold
    assert.deepEqual(result.mapping.keys, ["LControl", "LAlt"]);

    // mappings 现在恰好有 1 条
    assert.equal(session.getMappings().length, 1);
    assert.equal(session.getCurrentDraft(), null);
  });

  await test("新建录制确认 (Confirm)：包含非修饰键自动推断为 'click' 并原子写入 mappings", () => {
    const session = new DraftSession([], () => "id-enter");

    session.startNewDraft("xbutton2");
    const result = session.confirmDraft(["Enter"]);

    assert.equal(result.success, true);
    assert.equal(result.mapping.mode, "click"); // 包含 Enter -> click
    assert.deepEqual(result.mapping.keys, ["Enter"]);
    assert.equal(session.getMappings().length, 1);
  });

  await test("新建滚轮录制确认 (Confirm)：无论什么键均强制推断为 'click'", () => {
    const session = new DraftSession([], () => "id-wheel");

    session.startNewDraft("wheelup");
    // 录制纯修饰键
    const result = session.confirmDraft(["LShift"]);

    assert.equal(result.success, true);
    assert.equal(result.mapping.mode, "click"); // 滚轮强制 click
    assert.deepEqual(result.mapping.keys, ["LShift"]);
  });

  await test("重新录制已有映射：取消录制不影响原 keys，原 mapping 保持不变", () => {
    const initial = [
      { id: "m-orig", button: "xbutton1", mode: "hold", keys: ["LControl", "LAlt"] },
    ];
    const session = new DraftSession(initial);

    // 开启重新录制
    const draft = session.startEditDraft("m-orig");
    assert.equal(draft.isNew, false);
    assert.equal(draft.existingId, "m-orig");
    assert.deepEqual(draft.initialKeys, ["LControl", "LAlt"]);

    // 用户在录制界面反悔，按 Escape 取消
    session.cancelDraft();

    // 验证原 mapping 完全没有被修改
    const mappings = session.getMappings();
    assert.equal(mappings.length, 1);
    assert.equal(mappings[0].id, "m-orig");
    assert.deepEqual(mappings[0].keys, ["LControl", "LAlt"]);
    assert.equal(mappings[0].mode, "hold");
  });

  await test("重新录制已有映射：确认录制原子替换 keys，并对滚轮防御性修正", () => {
    const initial = [
      { id: "m-existing", button: "xbutton1", mode: "click", keys: ["Enter"] },
      { id: "m-wheel", button: "wheeldown", mode: "hold", keys: ["F5"] }, // 脏数据 hold
    ];
    const session = new DraftSession(initial);

    // 重新录制 m-existing 为 Ctrl + S
    session.startEditDraft("m-existing");
    const res1 = session.confirmDraft(["LControl", "S"]);
    assert.equal(res1.success, true);
    assert.deepEqual(res1.mapping.keys, ["LControl", "S"]);
    assert.equal(res1.mapping.mode, "click"); // 维持原本模式

    // 重新录制 m-wheel，应防御性修正 mode 为 click
    session.startEditDraft("m-wheel");
    const res2 = session.confirmDraft(["ArrowDown"]);
    assert.equal(res2.success, true);
    assert.deepEqual(res2.mapping.keys, ["ArrowDown"]);
    assert.equal(res2.mapping.mode, "click"); // 滚轮防御修正为 click
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

await suite("6. codeToToken 键盘事件兜底映射", async () => {
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
