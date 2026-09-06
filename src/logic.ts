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
  initialKeys: string[];
  suggestedMode?: "hold" | "click";
  slot?: "tap" | "hold" | "toggle";
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
  xbutton1: "侧键 · 后",
  xbutton2: "侧键 · 前",
  wheelup: "滚轮上",
  wheeldown: "滚轮下",
};

export const MODE_LABEL: Record<string, string> = {
  hold: "跟随按住",
  click: "单次触发",
  dual: "点按 / 长按",
  toggle: "切换保持",
};

export const MODE_DESC: Record<string, string> = {
  hold: "按下立刻注入，松开立刻释放。",
  click: "每按一下，完整触发一次快捷键",
  dual: "短按与长按可以绑两套键。只填长按则按下立刻跟随；两套都填时短按点触、长按超过阈值才跟随。",
  toggle: "按一次保持，再按一次释放",
};

const MODIFIER_KEY_SET = new Set([
  "ctrl",
  "control",
  "lcontrol",
  "rcontrol",
  "leftctrl",
  "rightctrl",
  "alt",
  "lalt",
  "ralt",
  "leftalt",
  "rightalt",
  "shift",
  "lshift",
  "rshift",
  "leftshift",
  "rightshift",
  "win",
  "lwin",
  "rwin",
  "leftwin",
  "rightwin",
  "meta",
  "lmeta",
  "rmeta",
  "cmd",
  "command",
]);

/**
 * 判断指定按键 token 是否为修饰键（Ctrl, Alt, Shift, Win / Meta）
 */
export function isModifierKey(key: string): boolean {
  if (!key) return false;
  return MODIFIER_KEY_SET.has(key.trim().toLowerCase());
}

/**
 * 获取修饰键权重，用于按 Ctrl -> Shift -> Alt -> Win -> 普通键 排序
 */
export function modifierWeight(key: string): number {
  const lower = key.trim().toLowerCase();
  if (
    lower === "ctrl" ||
    lower === "control" ||
    lower === "lcontrol" ||
    lower === "rcontrol" ||
    lower === "leftctrl" ||
    lower === "rightctrl"
  ) {
    return 10;
  }
  if (
    lower === "shift" ||
    lower === "lshift" ||
    lower === "rshift" ||
    lower === "leftshift" ||
    lower === "rightshift"
  ) {
    return 20;
  }
  if (
    lower === "alt" ||
    lower === "lalt" ||
    lower === "ralt" ||
    lower === "leftalt" ||
    lower === "rightalt"
  ) {
    return 30;
  }
  if (
    lower === "win" ||
    lower === "lwin" ||
    lower === "rwin" ||
    lower === "leftwin" ||
    lower === "rightwin" ||
    lower === "meta" ||
    lower === "lmeta" ||
    lower === "rmeta" ||
    lower === "cmd" ||
    lower === "command"
  ) {
    return 40;
  }
  return 100;
}

/**
 * 规范化按键 Chord：去重、过滤空项，并按标准修饰键次序排序
 */
export function normalizeKeyChord(keys: string[]): string[] {
  if (!keys || !Array.isArray(keys)) return [];
  const deduped: string[] = [];
  for (const k of keys) {
    if (k && typeof k === "string" && k.trim().length > 0 && !deduped.includes(k)) {
      deduped.push(k);
    }
  }
  deduped.sort((a, b) => {
    const wa = modifierWeight(a);
    const wb = modifierWeight(b);
    if (wa !== wb) {
      return wa - wb;
    }
    return a.localeCompare(b);
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
export function inferDefaultMode(keys: string[], button?: string): "hold" | "click" {
  if (button === "wheelup" || button === "wheeldown") {
    return "click";
  }
  if (!keys || keys.length === 0) {
    return "click";
  }
  const allModifiers = keys.every(isModifierKey);
  return allModifiers ? "hold" : "click";
}

/**
 * 与现有 main.ts 兼容的推断别名函数
 */
export function inferTriggerMode(button: string, keys: string[]): "hold" | "click" {
  return inferDefaultMode(keys, button);
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
  if (button === "left" || button === "right") {
    return false;
  }
  if (button === "wheelup" || button === "wheeldown") {
    return mode === "click";
  }
  if (button === "middle" || button === "xbutton1" || button === "xbutton2") {
    return mode === "hold" || mode === "click" || mode === "toggle" || mode === "dual";
  }
  return false;
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
  const keys = Array.isArray(raw.keys) ? [...raw.keys] : [];
  const tap = Array.isArray(raw.tap_keys) ? [...raw.tap_keys] : [];
  const hold = Array.isArray(raw.hold_keys) ? [...raw.hold_keys] : [];
  if (raw.button === "wheelup" || raw.button === "wheeldown") {
    const t = tap.length ? tap : keys;
    return { ...raw, mode: "click", keys: t, tap_keys: t, hold_keys: [] };
  }
  if (raw.mode === "toggle") {
    return { ...raw, mode: "toggle", keys, tap_keys: [], hold_keys: [] };
  }
  if (!tap.length && !hold.length && keys.length) {
    if (raw.mode === "hold") {
      return { ...raw, mode: "dual", keys, tap_keys: [], hold_keys: keys };
    }
    return { ...raw, mode: "dual", keys, tap_keys: keys, hold_keys: [] };
  }
  const nextKeys = keys.length ? keys : tap.length ? tap : hold;
  return { ...raw, mode: "dual", keys: nextKeys, tap_keys: tap, hold_keys: hold };
}

/**
 * 清洗和校验 mappings（用于启动加载时过滤非法映射）
 */
export function sanitizeMappings(rawMappings: Mapping[]): Mapping[] {
  const seenButtons = new Set<string>();
  return (rawMappings ?? [])
    .filter((m) => {
      // 严禁映射左键与右键
      if (m.button === "left" || m.button === "right") return false;
      if (seenButtons.has(m.button)) return false;
      seenButtons.add(m.button);
      return true;
    })
    .map((m) => normalizeMapping(m));
}

/**
 * 事务化录制状态机管理器 (Draft Mapping Session)
 */
export class DraftSession {
  private mappings: Mapping[];
  private currentDraft: DraftMapping | null = null;
  private idGenerator: () => string;

  constructor(
    initialMappings: Mapping[] = [],
    idGenerator: () => string = () => `map-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`
  ) {
    this.mappings = initialMappings.map((m) => ({ ...m, keys: [...m.keys] }));
    this.idGenerator = idGenerator;
  }

  /** 获取当前活跃的 mappings 副本 */
  getMappings(): readonly Mapping[] {
    return this.mappings;
  }

  /** 获取当前录制草稿副本 */
  getCurrentDraft(): DraftMapping | null {
    return this.currentDraft
      ? { ...this.currentDraft, initialKeys: [...this.currentDraft.initialKeys] }
      : null;
  }

  /**
   * 开启新按键的录制草稿
   * - 严禁为 left / right 开启录制
   * - 创建草稿阶段绝不污染现有 mappings
   */
  startNewDraft(button: string): DraftMapping {
    if (button === "left" || button === "right") {
      throw new Error(`左/右键 (${button}) 仅用于识别，为防止误锁系统，不支持映射。`);
    }
    const draft: DraftMapping = {
      button,
      isNew: true,
      initialKeys: [],
    };
    this.currentDraft = draft;
    return { ...draft };
  }

  /**
   * 为已有映射开启重新录制草稿
   * - 保存已有按键状态的快照，以便取消时完整保留
   */
  startEditDraft(mappingId: string): DraftMapping {
    const existing = this.mappings.find((m) => m.id === mappingId);
    if (!existing) {
      throw new Error(`Mapping not found: ${mappingId}`);
    }
    const draft: DraftMapping = {
      button: existing.button,
      isNew: false,
      existingId: existing.id,
      initialKeys: [...existing.keys],
    };
    this.currentDraft = draft;
    return { ...draft };
  }

  /**
   * 取消录制（Cancel / Esc / Blur）
   * - 直接抛弃草稿，mappings 保持原状，绝不遗留空 mapping
   */
  cancelDraft(): boolean {
    if (!this.currentDraft) return false;
    this.currentDraft = null;
    return true;
  }

  /**
   * 确认录制（Confirm）
   * - 若录制键为空，直接废弃草稿并不写入 mappings
   * - 若是新建映射：推断默认模式，生成新 Mapping 并原子追加进 mappings
   * - 若是编辑已有映射：原子替换 keys，滚轮自动防御修正为 click
   */
  confirmDraft(recordedKeys: string[]): {
    success: boolean;
    mapping?: Mapping;
    reason?: string;
  } {
    if (!this.currentDraft) {
      return { success: false, reason: "NO_ACTIVE_DRAFT" };
    }

    const normalizedKeys = normalizeKeyChord(recordedKeys);
    if (normalizedKeys.length === 0) {
      this.cancelDraft();
      return { success: false, reason: "EMPTY_KEYS" };
    }

    if (this.currentDraft.isNew) {
      const mode = inferDefaultMode(normalizedKeys, this.currentDraft.button);
      const newMapping: Mapping = {
        id: this.idGenerator(),
        button: this.currentDraft.button,
        mode,
        keys: normalizedKeys,
      };
      this.mappings.push(newMapping);
      this.currentDraft = null;
      return { success: true, mapping: newMapping };
    } else {
      const existing = this.mappings.find((m) => m.id === this.currentDraft?.existingId);
      if (!existing) {
        this.currentDraft = null;
        return { success: false, reason: "EXISTING_MAPPING_NOT_FOUND" };
      }
      // 原子替换按键
      existing.keys = normalizedKeys;
      // 滚轮防御修正
      if (
        (existing.button === "wheelup" || existing.button === "wheeldown") &&
        existing.mode !== "click"
      ) {
        existing.mode = "click";
      }
      this.currentDraft = null;
      return { success: true, mapping: existing };
    }
  }
}
