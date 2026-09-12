/**
 * Mouse Insight - 前端核心逻辑与状态机
 * 包含：修饰键判断、快捷键规范化、默认触发模式推断、按钮能力矩阵以及事务化录制状态机。
 */

export type TriggerMode = "hold" | "click" | "toggle" | "dual";

export type Mapping = {
  id: string;
  button: string;
  mode: TriggerMode | string;
  keys: string[];
  tap_keys?: string[];
  hold_keys?: string[];
  label?: string;
};

export type DraftMapping = {
  button: string;
  isNew: boolean;
  existingId?: string;
  slot?: "tap" | "hold" | "toggle";
  gen?: number;
};

export const MAPPABLE_BUTTONS = [
  "xbutton1",
  "xbutton2",
  "middle",
  "wheelup",
  "wheeldown",
] as const;

export const BUTTON_LABEL: Record<string, string> = {
  left: "左键",
  right: "右键",
  middle: "中键",
  xbutton1: "后侧键",
  xbutton2: "前侧键",
  wheelup: "滚轮上",
  wheeldown: "滚轮下",
};

export const MODE_LABEL: Record<string, string> = {
  hold: "跟随按住",
  click: "单次触发",
  dual: "短按 / 长按",
  toggle: "切换保持",
};

export const MODE_DESC: Record<string, string> = {
  hold: "按住生效 · 松开释放",
  click: "每按一次触发一次",
  dual: "短按触发一套 · 按住 0.4s 切换长按",
  toggle: "按下开 · 再按关",
};

/** Modifier aliases → canonical token (left side for bare names). */
export const CANONICAL_TOKENS: Record<string, string> = {
  ctrl: "LControl",
  control: "LControl",
  lcontrol: "LControl",
  leftctrl: "LControl",
  leftcontrol: "LControl",
  rctrl: "RControl",
  rcontrol: "RControl",
  rightctrl: "RControl",
  rightcontrol: "RControl",
  shift: "LShift",
  lshift: "LShift",
  leftshift: "LShift",
  rshift: "RShift",
  rightshift: "RShift",
  alt: "LAlt",
  lalt: "LAlt",
  leftalt: "LAlt",
  option: "LAlt",
  loption: "LAlt",
  leftoption: "LAlt",
  ralt: "RAlt",
  rightalt: "RAlt",
  roption: "RAlt",
  rightoption: "RAlt",
  win: "LWin",
  lwin: "LWin",
  leftwin: "LWin",
  meta: "LWin",
  lmeta: "LWin",
  cmd: "LWin",
  command: "LWin",
  lcommand: "LWin",
  leftcommand: "LWin",
  rwin: "RWin",
  rightwin: "RWin",
  rmeta: "RWin",
  rcmd: "RWin",
  rcommand: "RWin",
  rightcommand: "RWin",
};

export function canonicalizeToken(token: string): string {
  if (!token) return token;
  const t = token.trim();
  return CANONICAL_TOKENS[t.toLowerCase()] ?? t;
}

const CANONICAL_MODIFIERS = new Set([
  "LControl",
  "RControl",
  "LShift",
  "RShift",
  "LAlt",
  "RAlt",
  "LWin",
  "RWin",
]);

/**
 * 判断指定按键 token 是否为修饰键（Ctrl, Alt, Shift, Win / Meta）
 */
export function isModifierKey(key: string): boolean {
  if (!key) return false;
  return CANONICAL_MODIFIERS.has(canonicalizeToken(key));
}

/**
 * 获取修饰键权重，用于按 Ctrl -> Shift -> Alt -> Win -> 普通键 排序
 */
export function modifierWeight(key: string): number {
  const c = canonicalizeToken(key);
  if (c === "LControl" || c === "RControl") return 10;
  if (c === "LShift" || c === "RShift") return 20;
  if (c === "LAlt" || c === "RAlt") return 30;
  if (c === "LWin" || c === "RWin") return 40;
  return 100;
}

/**
 * 规范化按键 Chord：别名归一、去重、过滤空项，并按标准修饰键次序排序
 */
export function normalizeKeyChord(keys: string[]): string[] {
  if (!keys || !Array.isArray(keys)) return [];
  const deduped: string[] = [];
  for (const k of keys) {
    if (!k || typeof k !== "string" || k.trim().length === 0) continue;
    const c = canonicalizeToken(k);
    if (!deduped.includes(c)) deduped.push(c);
  }
  deduped.sort((a, b) => {
    const wa = modifierWeight(a);
    const wb = modifierWeight(b);
    if (wa !== wb) {
      return wa - wb;
    }
    return a < b ? -1 : a > b ? 1 : 0;
  });
  return deduped;
}

/**
 * 默认触发模式推断规则
 * 规则：
 * 1. 滚轮按键（wheelup / wheeldown）无条件强制为 "click"
 * 2. 空按键列表默认为 "click"
 * 3. 纯修饰键（所有键均为 isModifierKey）推断为 "hold"（跟随按住）
 * 4. 包含任何非修饰键（如 Enter, Space, F5 等）推断为 "click"（单次触发）
 * 5. "toggle" 绝不自动推荐
 */
export function inferTriggerMode(button: string, keys: string[]): "hold" | "click" {
  if (button === "wheelup" || button === "wheeldown") {
    return "click";
  }
  if (!keys || keys.length === 0) {
    return "click";
  }
  const allModifiers = keys.every(isModifierKey);
  return allModifiers ? "hold" : "click";
}

/** 兼容别名：语义同 inferTriggerMode，参数顺序为历史遗留。 */
export function inferDefaultMode(keys: string[], button?: string): "hold" | "click" {
  return inferTriggerMode(button ?? "", keys);
}

/**
 * 把 KeyboardEvent.code 映射成引擎 token。录制时作为 LL 钩子的兜底，
 * 避免钩子没挂上时页面 preventDefault 把按键吃掉却什么都不录。
 */
export function codeToToken(code: string): string | null {
  if (!code) return null;
  const mapped: Record<string, string> = {
    ControlLeft: "LControl",
    ControlRight: "RControl",
    AltLeft: "LAlt",
    AltRight: "RAlt",
    ShiftLeft: "LShift",
    ShiftRight: "RShift",
    MetaLeft: "LWin",
    MetaRight: "RWin",
    Space: "Space",
    Enter: "Enter",
    NumpadEnter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    Minus: "Minus",
    Equal: "Equal",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Backquote: "Backquote",
    BracketLeft: "BracketLeft",
    Backslash: "Backslash",
    BracketRight: "BracketRight",
    Quote: "Quote",
    Semicolon: "Semicolon",
    NumpadMultiply: "NumpadMultiply",
    NumpadAdd: "NumpadAdd",
    NumpadSubtract: "NumpadSubtract",
    NumpadDecimal: "NumpadDecimal",
    NumpadDivide: "NumpadDivide",
  };
  if (mapped[code]) return mapped[code];
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return code;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return null;
}

/**
 * 按钮能力矩阵规则检查
 * - left / right：不允许建立任何映射（仅用于硬件检测）
 * - wheelup / wheeldown：仅允许 "click"，不允许 "hold" 或 "toggle"
 * - middle, xbutton1, xbutton2：允许 "hold", "click", "toggle"
 */
export function isButtonAllowedForMode(button: string, mode: string): boolean {
  return (getAllowedModesForButton(button) as string[]).includes(mode);
}

/**
 * 获取指定按钮所支持的模式列表
 */
export function getAllowedModesForButton(button: string): ("dual" | "toggle" | "click")[] {
  if (button === "left" || button === "right") {
    return [];
  }
  if (button === "wheelup" || button === "wheeldown") {
    return ["click"];
  }
  if (button === "middle" || button === "xbutton1" || button === "xbutton2") {
    return ["dual", "toggle"];
  }
  return [];
}

export function normalizeMapping(raw: Mapping): Mapping {
  const keys = normalizeKeyChord(raw.keys);
  const tap = normalizeKeyChord(raw.tap_keys ?? []);
  const hold = normalizeKeyChord(raw.hold_keys ?? []);
  // Explicit slots are authoritative, including an intentionally empty slot.
  // Legacy "dual"+"keys" rows have no slots and migrate into tap_keys.
  const hasSlots = tap.length > 0 || hold.length > 0;
  if (raw.button === "wheelup" || raw.button === "wheeldown") {
    const t = hasSlots ? tap : keys;
    return { ...raw, mode: "click", keys: [...t], tap_keys: [...t], hold_keys: [...hold] };
  }
  if (raw.mode === "toggle") {
    return { ...raw, mode: "toggle", keys: [...keys], tap_keys: [...tap], hold_keys: [...hold] };
  }
  if (!hasSlots && keys.length) {
    if (raw.mode === "hold") {
      return { ...raw, mode: "dual", keys: [...keys], tap_keys: [], hold_keys: [...keys] };
    }
    return { ...raw, mode: "dual", keys: [...keys], tap_keys: [...keys], hold_keys: [] };
  }
  const nextKeys = tap.length ? tap : hold;
  return { ...raw, mode: "dual", keys: [...nextKeys], tap_keys: [...tap], hold_keys: [...hold] };
}

/**
 * 清洗和校验 mappings（用于启动加载时过滤非法映射）
 */
export function sanitizeMappings(rawMappings: Mapping[]): Mapping[] {
  const seenButtons = new Set<string>();
  return (rawMappings ?? [])
    .filter((m) => {
      // 严禁映射左键与右键
      if (!m || !MAPPABLE_BUTTONS.some((button) => button === m.button)) return false;
      if (seenButtons.has(m.button)) return false;
      seenButtons.add(m.button);
      return true;
    })
    .map((m) => normalizeMapping(m));
}

/**
 * 读取某模式槽位当前录制的按键组合
 * - "tap"：优先 tap_keys；历史 click 映射回退 keys
 * - "hold"：hold_keys
 * - "toggle"：keys
 */
export function keysForSlot(m: Mapping, slot: "tap" | "hold" | "toggle"): string[] {
  if (slot === "hold") return [...(m.hold_keys ?? [])];
  if (slot === "tap") return [...(m.tap_keys ?? (m.mode === "click" ? m.keys : []))];
  return [...(m.keys ?? [])];
}

/** 目标按键是否已被另一条映射占用 */
export function hasButtonConflict(all: Mapping[], id: string, button: string): boolean {
  return all.some((other) => other.id !== id && other.button === button);
}

/**
 * 切换映射的目标按键；冲突返回 null。
 * 改到滚轮时强制 click 并把 keys 回填进 tap_keys（hold_keys 数据保留）。
 */
export function applyButtonChange(
  m: Mapping,
  nextButton: string,
  all: Mapping[]
): Mapping | null {
  if (hasButtonConflict(all, m.id, nextButton)) return null;
  const next: Mapping = { ...m, button: nextButton };
  if (nextButton === "wheelup" || nextButton === "wheeldown") {
    next.mode = "click";
    next.tap_keys = next.tap_keys?.length ? next.tap_keys : [...next.keys];
  }
  return normalizeMapping(next);
}

/**
 * 切换触发模式，不清空另一槽位（P1-25 保槽语义）：
 * - toggle：keys 回填自 tap/hold/keys，tap_keys 与 hold_keys 原样保留
 * - click：tap_keys 回填自 tap/keys
 * - dual：两槽皆空时按 keys 推断进 tap 或 hold
 */
export function applyModeChange(m: Mapping, mode: "toggle" | "click" | "dual"): Mapping {
  const next: Mapping = { ...m };
  if (mode === "toggle") {
    next.mode = "toggle";
    next.keys = next.tap_keys?.length
      ? [...next.tap_keys]
      : next.hold_keys?.length
        ? [...next.hold_keys]
        : [...next.keys];
  } else if (mode === "click") {
    next.mode = "click";
    next.tap_keys = next.tap_keys?.length ? [...next.tap_keys] : [...next.keys];
  } else {
    next.mode = "dual";
    if (!next.tap_keys?.length && !next.hold_keys?.length && next.keys.length) {
      next.hold_keys = inferTriggerMode(next.button, next.keys) === "hold" ? [...next.keys] : [];
      next.tap_keys = next.hold_keys.length ? [] : [...next.keys];
    }
  }
  return normalizeMapping(next);
}

/**
 * 从指定槽位删除一个键；非 toggle 槽删除后 keys 回填 tap 或 hold。
 */
export function removeKeyFromSlot(
  m: Mapping,
  slot: "tap" | "hold" | "toggle",
  key: string
): Mapping {
  const next: Mapping = { ...m };
  if (slot === "hold") {
    next.hold_keys = (m.hold_keys ?? []).filter((k) => k !== key);
  } else if (slot === "tap") {
    next.tap_keys = (m.tap_keys ?? []).filter((k) => k !== key);
  } else {
    next.keys = m.keys.filter((k) => k !== key);
  }
  if (slot !== "toggle") {
    next.keys = next.tap_keys?.length ? [...next.tap_keys] : [...(next.hold_keys ?? [])];
  }
  return next;
}
