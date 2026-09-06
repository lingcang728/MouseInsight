import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  type Mapping,
  type DraftMapping,
  BUTTON_LABEL,
  MAPPABLE_BUTTONS,
  MODE_LABEL,
  MODE_DESC,
  inferTriggerMode,
  normalizeKeyChord,
  getAllowedModesForButton,
  sanitizeMappings,
  codeToToken,
  normalizeMapping,
} from "./logic";

type Pulse = {
  button: string;
  down: boolean;
  t: number;
  swallowed: boolean;
};

type SendReport = {
  timestamp: number;
  expected: number;
  inserted: number;
  win32_error: number;
  is_uipi_blocked: boolean;
};

type Snapshot = {
  config: {
    schema_version: number;
    theme: string;
    autostart: boolean;
    paused: boolean;
    mappings: Mapping[];
  };
  xmbc_running: boolean;
  last: Pulse | null;
  listening: boolean;
  is_portable: boolean;
  config_dir: string;
};

type RuntimeState = {
  active: boolean;
  mode?: string;
};

let mappings: Mapping[] = [];
let selectedMappingId: string | null = null;
let currentDraft: DraftMapping | null = null;
let recordBuf: string[] = [];
let isPaused = false;
const runtimeStates: Map<string, RuntimeState> = new Map();

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function prettyKeys(keys: string[]): string[] {
  if (!keys.length) return ["空"];
  return keys.map((k) => {
    if (k === "LControl") return "Left Ctrl";
    if (k === "RControl") return "Right Ctrl";
    if (k === "LAlt") return "Left Alt";
    if (k === "RAlt") return "Right Alt";
    if (k === "LShift") return "Left Shift";
    if (k === "RShift") return "Right Shift";
    if (k === "LWin") return "Win";
    return k;
  });
}

function renderKeys(
  el: HTMLElement,
  keys: string[],
  opts: { dimEmpty?: boolean; removable?: boolean } = {}
) {
  const dimEmpty = opts.dimEmpty !== false;
  const removable = !!opts.removable;
  el.replaceChildren();
  if (!keys.length) {
    const span = document.createElement("span");
    span.className = `key${dimEmpty ? " dim" : ""}`;
    span.textContent = "空";
    el.appendChild(span);
    return;
  }
  prettyKeys(keys).forEach((label, i) => {
    const chip = document.createElement("span");
    chip.className = "key-chip";
    const text = document.createElement("span");
    text.className = "k";
    text.textContent = label;
    chip.appendChild(text);
    if (removable) {
      const x = document.createElement("button");
      x.type = "button";
      x.className = "key-x";
      x.dataset.removeKey = keys[i];
      x.setAttribute("aria-label", `删除 ${label}`);
      x.textContent = "×";
      chip.appendChild(x);
    }
    el.appendChild(chip);
  });
}

function applyTheme(theme: string) {
  document.documentElement.dataset.theme = theme;
  $("btn-theme").textContent = theme === "dark" ? "换成浅色" : "换成深色";
}

function setPausedUi(paused: boolean) {
  isPaused = paused;
  $("btn-pause").textContent = paused ? "继续映射" : "暂停映射";
  if (paused) {
    runtimeStates.clear();
    updateAllRuntimePills();
  }
}

function getStatusPill(m: Mapping): { text: string; className: string } | null {
  if (isPaused) {
    if (m.mode === "toggle") {
      return { text: "已关闭", className: "pill pill-off status-pill inactive" };
    }
    return null;
  }

  const state = runtimeStates.get(m.button);
  const isActive = !!state?.active;

  if (m.mode === "toggle") {
    return isActive
      ? { text: "保持中", className: "pill pill-on status-pill active" }
      : { text: "已关闭", className: "pill pill-off status-pill inactive" };
  } else if (m.mode === "hold" || ((m.mode === "dual" || !m.mode) && (m.hold_keys?.length ?? 0))) {
    return isActive
      ? { text: "按住中", className: "pill pill-hold status-pill active" }
      : null;
  }
  return null;
}

function updateAllRuntimePills() {
  mappings.forEach((m) => {
    const article = document.querySelector(`.map[data-id="${m.id}"]`);
    if (!article) return;
    const pill = article.querySelector(".pill, .status-pill") as HTMLElement | null;
    if (!pill) return;
    const pillData = getStatusPill(m);
    if (pillData) {
      pill.className = pillData.className;
      pill.textContent = pillData.text;
    } else {
      pill.className = "pill status-pill hidden";
      pill.textContent = "";
    }
  });
}

let lastDotCol: number | undefined;

function paintDots(button: string) {
  const host = $("dots");
  if (!host.childElementCount) {
    const fragment = document.createDocumentFragment();
    for (let i = 0; i < 16 * 4; i++) {
      const dot = document.createElement("i");
      dot.className = "dot";
      fragment.appendChild(dot);
    }
    host.appendChild(fragment);
    lastDotCol = undefined;
  }
  const col = { xbutton1: 1, xbutton2: 3, middle: 7, wheelup: 9, wheeldown: 10, left: 12, right: 14 }[
    button
  ];
  if (col === lastDotCol) return;
  const dots = host.children;
  for (let i = 0; i < dots.length; i++) {
    const d = dots[i] as HTMLElement;
    const c = i % 16;
    const lit = col !== undefined && Math.abs(c - col) < 2;
    const hot = c === col;
    d.className = hot ? "dot hot" : lit ? "dot lit" : "dot";
  }
  lastDotCol = col;
}

function highlightMouse(button: string) {
  document.querySelectorAll(".mouse .zone").forEach((z) => z.classList.remove("on"));
  const id =
    button === "wheelup" || button === "wheeldown" ? "zone-middle" : `zone-${button}`;
  document.getElementById(id)?.classList.add("on");
}

function renderDock() {
  const selected = mappings.find((m) => m.id === selectedMappingId) ?? mappings[0];
  if (!selected) {
    $("dock-lead").textContent = "还没有绑定";
    renderKeys($("dock-keys"), []);
    const hintEl = $("dock-hint");
    if (hintEl) {
      hintEl.textContent = "键盘任意键都能录。Typeless 听写可录 Left Ctrl + Left Alt。";
    }
    return;
  }
  const mode = selected.mode === "toggle" ? "toggle" : "dual";
  $("dock-lead").textContent = `${BUTTON_LABEL[selected.button] ?? selected.button} · ${MODE_LABEL[mode] ?? mode}`;
  const shown =
    selected.mode === "toggle"
      ? selected.keys
      : (selected.tap_keys?.length ? selected.tap_keys : selected.hold_keys) ?? selected.keys;
  renderKeys($("dock-keys"), shown ?? []);
  const hintEl = $("dock-hint");
  if (hintEl) {
    hintEl.textContent = MODE_DESC[mode] ?? MODE_DESC[selected.mode] ?? "";
  }
}

function makeComboRow(
  slot: "tap" | "hold" | "toggle",
  label: string,
  keys: string[]
): HTMLElement {
  const row = document.createElement("div");
  row.className = "gesture";
  const lab = document.createElement("span");
  lab.className = "gesture-label";
  lab.textContent = label;
  const combo = document.createElement("div");
  combo.className = "combo";
  combo.dataset.slot = slot;
  renderKeys(combo, keys, { removable: true });
  const rec = document.createElement("button");
  rec.type = "button";
  rec.className = "ghost map-btn-record";
  rec.dataset.act = "record";
  rec.dataset.slot = slot;
  rec.textContent = keys.length ? "重录" : "录制";
  row.append(lab, combo, rec);
  return row;
}

function renderMaps(liveButton?: string) {
  const host = $("maps");
  if (!mappings.length) {
    host.replaceChildren();
    const p = document.createElement("p");
    p.className = "meta";
    p.textContent = "还没有映射。先听一颗键。";
    host.appendChild(p);
    selectedMappingId = null;
    renderDock();
    return;
  }

  if (!mappings.some((m) => m.id === selectedMappingId)) {
    selectedMappingId = mappings[0].id;
  }

  host.replaceChildren();
  mappings.forEach((m) => {
    const article = document.createElement("article");
    const isLive = m.button === liveButton;
    const isSelected = m.id === selectedMappingId;
    article.className = `map${isLive ? " live" : ""}${isSelected ? " selected" : ""}`;
    article.dataset.id = m.id;
    article.dataset.button = m.button;

    const row = document.createElement("div");
    row.className = "map-primary-row";

    const selButton = document.createElement("select");
    selButton.dataset.k = "button";
    selButton.className = "map-sel-btn";
    MAPPABLE_BUTTONS.forEach((btnKey) => {
      const opt = document.createElement("option");
      opt.value = btnKey;
      const isOccupied = mappings.some((other) => other.id !== m.id && other.button === btnKey);
      const labelText = BUTTON_LABEL[btnKey] ?? btnKey;
      opt.textContent = isOccupied ? `${labelText} (已绑定)` : labelText;
      opt.selected = m.button === btnKey;
      opt.disabled = isOccupied;
      selButton.appendChild(opt);
    });

    const selMode = document.createElement("select");
    selMode.dataset.k = "mode";
    selMode.className = "map-sel-mode";
    const availableModes = getAllowedModesForButton(m.button);
    const uiMode = m.mode === "toggle" ? "toggle" : availableModes[0];
    availableModes.forEach((modeKey) => {
      const opt = document.createElement("option");
      opt.value = modeKey;
      opt.textContent = MODE_LABEL[modeKey] ?? modeKey;
      opt.selected = uiMode === modeKey;
      selMode.appendChild(opt);
    });

    const pill = document.createElement("span");
    const pillData = getStatusPill(m);
    pill.className = pillData ? pillData.className : "pill status-pill hidden";
    pill.textContent = pillData ? pillData.text : "";

    const btnDel = document.createElement("button");
    btnDel.type = "button";
    btnDel.className = "danger";
    btnDel.dataset.act = "del";
    btnDel.setAttribute("aria-label", "删除");
    btnDel.textContent = "删除";

    row.append(selButton, selMode, pill, btnDel);
    article.append(row);

    const isWheel = m.button === "wheelup" || m.button === "wheeldown";
    if (m.mode === "toggle") {
      article.append(makeComboRow("toggle", "组合", m.keys ?? []));
    } else if (isWheel) {
      article.append(makeComboRow("tap", "短按", m.tap_keys ?? m.keys ?? []));
    } else {
      article.append(makeComboRow("tap", "短按", m.tap_keys ?? []));
      article.append(makeComboRow("hold", "长按", m.hold_keys ?? []));
    }

    const desc = document.createElement("div");
    desc.className = "map-desc mode-hint";
    desc.textContent = MODE_DESC[uiMode] ?? MODE_DESC[m.mode] ?? "";
    article.append(desc);
    host.appendChild(article);
  });

  renderDock();
}

async function persist() {
  try {
    mappings = sanitizeMappings(mappings);
    await invoke("save_mappings", { mappings });
  } catch (err) {
    showAlert(`保存配置失败: ${String(err)}`);
  }
}

function showAlert(msg: string) {
  const banner = $("alert-banner");
  if (banner) {
    banner.textContent = msg;
    banner.classList.remove("hidden");
  }
}

function keysForSlot(m: Mapping, slot: "tap" | "hold" | "toggle"): string[] {
  if (slot === "hold") return [...(m.hold_keys ?? [])];
  if (slot === "tap") return [...(m.tap_keys ?? (m.mode === "click" ? m.keys : []))];
  return [...(m.keys ?? [])];
}

async function openRecordForExisting(id: string, slot: "tap" | "hold" | "toggle" = "tap") {
  const m = mappings.find((x) => x.id === id);
  if (!m) return;
  const initialKeys = keysForSlot(m, slot);
  currentDraft = {
    button: m.button,
    isNew: false,
    existingId: m.id,
    initialKeys,
    slot,
  };
  recordBuf = [...initialKeys];
  $("record-mask").classList.remove("hidden");
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      slot === "hold" ? "录长按组合键" : slot === "tap" ? "录短按组合键" : "录切换组合键";
  }
  renderKeys($("record-keys"), recordBuf, { dimEmpty: false, removable: true });
  await invoke("arm_record");
}

async function openRecordForNew(button: string, slot: "tap" | "hold" | "toggle" = "tap") {
  currentDraft = {
    button,
    isNew: true,
    initialKeys: [],
    slot,
  };
  recordBuf = [];
  $("record-mask").classList.remove("hidden");
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      slot === "hold" ? "录长按组合键" : slot === "tap" ? "录短按组合键" : "录切换组合键";
  }
  renderKeys($("record-keys"), [], { removable: true });
  await invoke("arm_record");
}

async function closeRecord() {
  currentDraft = null;
  recordBuf = [];
  $("record-mask").classList.add("hidden");
  try {
    await invoke("disarm_record");
  } catch (err) {
    console.error("disarm_record failed:", err);
  }
}

async function confirmRecord() {
  if (!currentDraft) {
    await closeRecord();
    return;
  }
  let keys: string[] = [];
  try {
    keys = await invoke<string[]>("take_record_keys");
  } catch (err) {
    console.error("take_record_keys error:", err);
  }
  const rawKeys = keys.length ? keys : recordBuf;
  const finalKeys = normalizeKeyChord(rawKeys);
  if (!finalKeys.length) {
    showAlert("未录入任何按键，录制已取消。");
    await closeRecord();
    return;
  }

  const slot = currentDraft.slot ?? (inferTriggerMode(currentDraft.button, finalKeys) === "hold" ? "hold" : "tap");
  if (currentDraft.isNew) {
    const newMapping: Mapping = normalizeMapping({
      id: crypto.randomUUID(),
      button: currentDraft.button,
      mode: slot === "toggle" ? "toggle" : "dual",
      keys: slot === "toggle" ? [...finalKeys] : [],
      tap_keys: slot === "tap" ? [...finalKeys] : [],
      hold_keys: slot === "hold" ? [...finalKeys] : [],
    });
    mappings.push(newMapping);
    selectedMappingId = newMapping.id;
  } else {
    const existing = mappings.find((x) => x.id === currentDraft?.existingId);
    if (existing) {
      if (slot === "toggle") {
        existing.mode = "toggle";
        existing.keys = [...finalKeys];
        existing.tap_keys = [];
        existing.hold_keys = [];
      } else if (slot === "hold") {
        existing.hold_keys = [...finalKeys];
        existing.mode = existing.mode === "toggle" ? "dual" : existing.mode || "dual";
        if (existing.mode !== "toggle") existing.mode = "dual";
      } else {
        existing.tap_keys = [...finalKeys];
        if (existing.mode !== "toggle") existing.mode = "dual";
      }
      mappings = mappings.map((x) => (x.id === existing.id ? normalizeMapping(existing) : x));
    }
  }

  await persist();
  renderMaps();
  await closeRecord();
}

async function boot() {
  const snap = await invoke<Snapshot>("get_snapshot");
  const rawMappings = snap.config.mappings ?? [];
  mappings = sanitizeMappings(rawMappings);

  if (mappings.length) {
    selectedMappingId = mappings[0].id;
  }
  if (JSON.stringify(rawMappings) !== JSON.stringify(mappings)) {
    await persist();
  }

  applyTheme(snap.config.theme || "dark");
  setPausedUi(!!snap.config.paused);
  const autostartBox = $("chk-autostart") as HTMLInputElement;
  try {
    const { isEnabled } = await import("@tauri-apps/plugin-autostart");
    const enabled = await isEnabled();
    autostartBox.checked = enabled;
    if (enabled !== !!snap.config.autostart) {
      await invoke("save_autostart", { on: enabled });
    }
  } catch {
    autostartBox.checked = !!snap.config.autostart;
  }
  $("xmbc-banner").classList.toggle("hidden", !snap.xmbc_running);

  const modeTag = snap.is_portable ? "[便携模式] " : "[标准安装] ";
  const pathEl = $("cfg-path");
  pathEl.textContent = `${modeTag}${snap.config_dir}`;
  pathEl.title = "点击在文件资源管理器中定位配置目录";
  pathEl.addEventListener("click", async () => {
    await invoke("open_config_dir");
  });

  paintDots("xbutton1");
  renderMaps();

  if (snap.last) {
    $("pulse-name").textContent = BUTTON_LABEL[snap.last.button] ?? snap.last.button;
    highlightMouse(snap.last.button);
    paintDots(snap.last.button);
  }

  let pendingPulse: Pulse | null = null;
  let pulseRaf = 0;
  const applyPulse = (p: Pulse) => {
    $("pulse-name").textContent = BUTTON_LABEL[p.button] ?? p.button;
    $("pulse-state").textContent = p.down
      ? p.swallowed
        ? "已拦截，改发快捷键"
        : "放行，Windows 原样处理"
      : "松开";
    $("pulse-ago").textContent = p.down ? "按下" : "松开";
    highlightMouse(p.button);
    paintDots(p.button);

    document.querySelectorAll(".map").forEach((el) => {
      el.classList.toggle("live", (el as HTMLElement).dataset.button === p.button && p.down);
    });

    if (p.down) {
      const active = mappings.find((m) => m.button === p.button);
      if (active && selectedMappingId !== active.id) {
        selectedMappingId = active.id;
        document.querySelectorAll(".map").forEach((el) => {
          el.classList.toggle("selected", (el as HTMLElement).dataset.id === active.id);
        });
        renderDock();
      }
    }

    const targetMap = mappings.find((m) => m.button === p.button);
    if (targetMap && targetMap.mode === "hold") {
      runtimeStates.set(p.button, { active: p.down, mode: "hold" });
      updateAllRuntimePills();
    }
  };
  await listen<Pulse>("mouse-pulse", (ev) => {
    pendingPulse = ev.payload;
    if (pulseRaf) return;
    pulseRaf = window.requestAnimationFrame(() => {
      pulseRaf = 0;
      if (pendingPulse) applyPulse(pendingPulse);
    });
  });

  await listen<{ button: string; active: boolean; mode?: string }>(
    "runtime-binding-changed",
    (ev) => {
      const { button, active, mode } = ev.payload;
      runtimeStates.set(button, { active, mode });
      updateAllRuntimePills();
    }
  );

  await listen<{ paused: boolean }>("engine-state-changed", (ev) => {
    setPausedUi(ev.payload.paused);
  });

  await listen<SendReport>("injection-error", (ev) => {
    const r = ev.payload;
    if (r.is_uipi_blocked) {
      showAlert("⚠️ 快捷键注入受阻：目标窗口可能以管理员权限运行（受 Windows UIPI 特权保护）。如有需要，请以管理员身份启动 Mouse Insight。");
    } else {
      showAlert(`⚠️ 按键注入失败 (成功 ${r.inserted}/${r.expected}, 错误码 ${r.win32_error})`);
    }
  });

  await listen<string>("listen-captured", async (ev) => {
    const capturedButton = ev.payload;

    // 严格限制：左键和右键仅用于识别，不支持映射
    if (capturedButton === "left" || capturedButton === "right") {
      showAlert("左/右键仅用于识别，为防止误锁系统，不支持映射。");
      $("listen-copy").textContent = `听到了：${BUTTON_LABEL[capturedButton] ?? capturedButton}（仅用于识别，不支持映射）。`;
      $("btn-listen").textContent = "开始听";
      document.querySelector(".listen-sheet")?.classList.remove("armed");
      return;
    }

    $("listen-copy").textContent = `听到了：${BUTTON_LABEL[capturedButton] ?? capturedButton}。请录键盘。`;
    $("btn-listen").textContent = "再听一颗";
    document.querySelector(".listen-sheet")?.classList.remove("armed");

    const existing = mappings.find((x) => x.button === capturedButton);
    const inferredSlot =
      capturedButton === "wheelup" || capturedButton === "wheeldown" ? "tap" : "hold";
    if (existing) {
      selectedMappingId = existing.id;
      renderMaps(capturedButton);
      const slot =
        existing.mode === "toggle" ? "toggle" : inferredSlot === "hold" && !(existing.hold_keys?.length) && (existing.tap_keys?.length) ? "tap" : inferredSlot;
      await openRecordForExisting(existing.id, slot);
    } else {
      await openRecordForNew(capturedButton, inferredSlot);
    }
  });

  await listen<string[]>("record-keys", (ev) => {
    recordBuf = ev.payload ?? [];
    renderKeys($("record-keys"), recordBuf, { removable: true });
  });

  await listen("record-cancel", () => {
    closeRecord();
  });
}

$("btn-theme").addEventListener("click", async () => {
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  applyTheme(next);
  await invoke("save_theme", { theme: next });
});

$("btn-pause").addEventListener("click", async () => {
  const next = !isPaused;
  setPausedUi(next);
  await invoke("save_paused", { paused: next });
});

$("btn-listen").addEventListener("click", async () => {
  $("listen-copy").textContent = "在听。按鼠标上你要绑定的那颗键（仅中键、侧键与滚轮支持绑定）。";
  $("btn-listen").textContent = "在听…";
  document.querySelector(".listen-sheet")?.classList.add("armed");
  await invoke("arm_listen");
});

$("btn-quit").addEventListener("click", async () => {
  await invoke("quit_app");
});

$("maps").addEventListener("change", async (e) => {
  const t = e.target as HTMLElement;
  const row = t.closest(".map") as HTMLElement | null;
  if (!row) return;
  const m = mappings.find((x) => x.id === row.dataset.id);
  if (!m) return;
  const sel = t as HTMLSelectElement;

  if (sel.dataset.k === "button") {
    const nextBtn = sel.value;
    const isConflict = mappings.some((other) => other.id !== m.id && other.button === nextBtn);
    if (isConflict) {
      sel.value = m.button;
      return;
    }
    m.button = nextBtn;
    if (nextBtn === "wheelup" || nextBtn === "wheeldown") {
      m.mode = "click";
      m.tap_keys = m.tap_keys?.length ? m.tap_keys : m.keys;
      m.hold_keys = [];
    }
  }
  if (sel.dataset.k === "mode") {
    if (sel.value === "toggle") {
      m.mode = "toggle";
      if (!m.keys.length) {
        m.keys = m.tap_keys?.length ? m.tap_keys : m.hold_keys ?? [];
      }
      m.tap_keys = [];
      m.hold_keys = [];
    } else if (sel.value === "click") {
      m.mode = "click";
      m.tap_keys = m.tap_keys?.length ? m.tap_keys : m.keys;
      m.hold_keys = [];
    } else {
      m.mode = "dual";
      if (!m.tap_keys?.length && !m.hold_keys?.length && m.keys.length) {
        m.hold_keys = inferTriggerMode(m.button, m.keys) === "hold" ? m.keys : [];
        m.tap_keys = m.hold_keys.length ? [] : m.keys;
      }
    }
  }

  // 清除该按键的旧 runtime 活跃状态
  runtimeStates.delete(m.button);
  await persist();
  renderMaps();
});

$("maps").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const row = t.closest(".map") as HTMLElement | null;
  if (!row) return;
  const id = row.dataset.id!;

  if (t.dataset.act === "del") {
    const target = mappings.find((m) => m.id === id);
    if (target) {
      runtimeStates.delete(target.button);
    }
    mappings = mappings.filter((m) => m.id !== id);
    if (selectedMappingId === id) {
      selectedMappingId = mappings[0]?.id ?? null;
    }
    await persist();
    renderMaps();
    return;
  }

  const removeKey = t.dataset.removeKey ?? t.closest("button")?.getAttribute("data-remove-key");
  if (removeKey) {
    const slot = (t.closest(".combo") as HTMLElement | null)?.dataset.slot as
      | "tap"
      | "hold"
      | "toggle"
      | undefined;
    const target = mappings.find((m) => m.id === id);
    if (target && slot) {
      if (slot === "hold") {
        target.hold_keys = (target.hold_keys ?? []).filter((k) => k !== removeKey);
      } else if (slot === "tap") {
        target.tap_keys = (target.tap_keys ?? []).filter((k) => k !== removeKey);
      } else {
        target.keys = target.keys.filter((k) => k !== removeKey);
      }
      await persist();
      renderMaps();
    }
    return;
  }

  if (t.dataset.act === "record") {
    const slot = (t.dataset.slot as "tap" | "hold" | "toggle") || "tap";
    selectedMappingId = id;
    renderMaps();
    openRecordForExisting(id, slot);
    return;
  }

  if (selectedMappingId !== id) {
    selectedMappingId = id;
    renderMaps();
  }
});

$("record-cancel").addEventListener("click", () => {
  closeRecord();
});

$("record-ok").addEventListener("click", async () => {
  await confirmRecord();
});

$("record-mask").addEventListener("click", (e) => {
  if (e.target === $("record-mask")) {
    closeRecord();
  }
});

$("record-keys").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const key = t.dataset.removeKey;
  if (!key || !currentDraft) return;
  e.preventDefault();
  e.stopPropagation();
  recordBuf = recordBuf.filter((k) => k !== key);
  renderKeys($("record-keys"), recordBuf, { removable: true });
  await invoke("remove_record_key", { key });
});

$("record-chips").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const key = t.dataset.key;
  if (!key) return;
  await invoke("add_record_key", { key });
});

window.addEventListener(
  "keydown",
  (e) => {
    if (!currentDraft) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeRecord();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    const token = codeToToken(e.code);
    if (!token) return;
    void invoke("add_record_key", { key: token });
  },
  true
);

$("chk-autostart").addEventListener("change", async (e) => {
  const on = (e.target as HTMLInputElement).checked;
  const { enable, disable } = await import("@tauri-apps/plugin-autostart");
  try {
    if (on) await enable();
    else await disable();
    await invoke("save_autostart", { on });
  } catch {
    (e.target as HTMLInputElement).checked = !on;
  }
});

function parseVer(v: string): number[] {
  return v
    .replace(/^v/i, "")
    .split(".")
    .map((n) => parseInt(n, 10) || 0);
}

function isNewer(remote: string, current: string): boolean {
  const a = parseVer(remote);
  const b = parseVer(current);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}

async function checkForUpdate(current: string) {
  const status = $("update-status");
  status.textContent = "正在检查…";
  try {
    let remoteVer = "";
    let remoteUrl = "https://github.com/lingcang728/MouseInsight/releases/latest";
    try {
      const r = await fetch(
        "https://github.com/lingcang728/MouseInsight/releases/latest/download/latest.json"
      );
      if (r.ok) {
        const j = await r.json();
        remoteVer = String(j.version ?? "").replace(/^v/i, "");
        if (j.url) remoteUrl = String(j.url);
      }
    } catch {
      /* fallback below */
    }
    if (!remoteVer) {
      const r = await fetch("https://api.github.com/repos/lingcang728/MouseInsight/releases/latest");
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const j = await r.json();
      remoteVer = String(j.tag_name ?? "").replace(/^v/i, "");
      if (j.html_url) remoteUrl = String(j.html_url);
    }
    if (!remoteVer) throw new Error("empty version");
    if (isNewer(remoteVer, current)) {
      status.textContent = `有新版本 v${remoteVer}`;
      await invoke("open_url", { url: remoteUrl });
    } else {
      status.textContent = "已是最新版";
    }
  } catch (err) {
    status.textContent = `检查失败：${String(err)}`;
  }
}

boot()
  .then(async () => {
    try {
      const ver = await invoke<string>("app_version");
      $("app-version").textContent = `v${ver}`;
      $("btn-check-update").addEventListener("click", () => {
        void checkForUpdate(ver);
      });
    } catch {
      $("app-version").textContent = "v0.1.3";
    }
  })
  .catch((err) => {
    console.error("MouseInsight 初始化异常:", err);
  });
