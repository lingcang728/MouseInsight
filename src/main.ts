import { isNewer, latestRelease, RELEASES_URL } from "./updates";
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
const collapsedMappings = new Set<string>();
let currentDraft: DraftMapping | null = null;
let recordBuf: string[] = [];
let isPaused = false;
const isMac = /Mac/i.test(navigator.platform);
let listening = false;
let hookReady = false;
let listenTimer = 0;
let confirmedMappings: Mapping[] = [];
let saveQueue: Promise<void> = Promise.resolve();
let saveRevision = 0;
let confirming = false;
let focusBeforeRecord: HTMLElement | null = null;
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
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
    if (k === "LWin") return isMac ? "⌘ Command" : "Left Win";
    if (k === "RWin") return isMac ? "Right Command" : "Right Win";
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
  $("btn-theme").textContent = theme === "dark" ? "浅色外观" : "深色外观";
}

function setPausedUi(paused: boolean) {
  isPaused = paused;
  $("engine-status").textContent = !hookReady ? "正在检查监听" : paused ? "映射已暂停" : "映射已启用";
  $("engine-status").classList.toggle("paused", paused);
  $("btn-pause").setAttribute("aria-pressed", String(paused));
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
    const article = document.querySelector(`.map[data-id="${CSS.escape(m.id)}"]`);
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
  const mode = selected.button.startsWith("wheel") ? "click" : selected.mode === "toggle" ? "toggle" : "dual";
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
  resetDeck();
  const host = $("maps");
  $("mapping-count").textContent = `${mappings.length} / 5 已配置`;
  document.querySelectorAll<HTMLButtonElement>("[data-add-button]").forEach((button) => {
    const existing = mappings.find((m) => m.button === button.dataset.addButton);
    button.textContent = `${existing ? "" : "＋ "}${BUTTON_LABEL[button.dataset.addButton!]}`;
    button.classList.toggle("active", existing?.id === selectedMappingId);
    button.setAttribute("aria-pressed", String(existing?.id === selectedMappingId));
  });
  if (!mappings.length) {
    host.replaceChildren();
    const p = document.createElement("p");
    p.className = "meta";
    p.className = "empty-state";
    p.textContent = "从一颗侧键开始。选择上方按键，或点击「识别鼠标键」，把常用操作放到指尖。";
    host.appendChild(p);
    selectedMappingId = null;
    renderDock();
    return;
  }

  if (!mappings.some((m) => m.id === selectedMappingId)) {
    selectedMappingId = mappings[0].id;
  }

  host.replaceChildren();
  [...mappings].sort((a, b) => Number(b.id === selectedMappingId) - Number(a.id === selectedMappingId)).forEach((m) => {
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
    selButton.setAttribute("aria-label", "鼠标按键");
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
    selMode.setAttribute("aria-label", "触发方式");
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

    const collapse = document.createElement("button");
    collapse.className = "ghost collapse-toggle";
    collapse.dataset.act = "collapse";
    collapse.textContent = collapsedMappings.has(m.id) ? "展开" : "折叠";
    collapse.setAttribute("aria-expanded", String(!collapsedMappings.has(m.id)));
    article.classList.toggle("collapsed", collapsedMappings.has(m.id));
    const grip = document.createElement("div");
    grip.className = "card-grip";
    grip.textContent = BUTTON_LABEL[m.button] + " · 拖动切换";
    article.append(grip);
    row.append(selButton, selMode, pill, collapse, btnDel);
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
  mappings = sanitizeMappings(mappings);
  const snapshot = structuredClone(mappings);
  const revision = ++saveRevision;
  renderMaps();
  $("save-status").textContent = "正在保存…";
  saveQueue = saveQueue.then(async () => {
    try {
      await invoke("save_mappings", { mappings: snapshot });
      confirmedMappings = snapshot;
      if (revision === saveRevision) $("save-status").textContent = "已保存 · 即时生效";
    } catch (err) {
      if (revision === saveRevision) {
        mappings = structuredClone(confirmedMappings);
        renderMaps();
        $("save-status").textContent = "保存失败 · 已恢复";
      }
      showAlert(`保存配置失败: ${String(err)}`);
    }
  });
  await saveQueue;
}

function selectMapping(id: string) {
  selectedMappingId = id;
  document.querySelectorAll<HTMLElement>(".map").forEach((el) => {
    el.classList.toggle("selected", el.dataset.id === id);
  });
  const selected = document.querySelector<HTMLElement>(`.map[data-id="${CSS.escape(id)}"]`);
  if (selected) $("maps").prepend(selected);
  document.querySelectorAll<HTMLElement>("[data-add-button]").forEach(button => {
    const active = mappings.find(m => m.id === id)?.button === button.dataset.addButton;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  renderDock();
}

function showRecorder() {
  focusBeforeRecord = document.activeElement as HTMLElement;
  document.querySelector<HTMLElement>(".app")!.inert = true;
  $("record-mask").classList.remove("hidden");
  $("record-cancel").focus();
}

async function startRecorder() {
  try {
    await invoke("arm_record");
  } catch (err) {
    await closeRecord();
    showAlert(`无法开始录制：${String(err)}`);
  }
}

async function stopListening() {
  listening = false;
  clearTimeout(listenTimer);
  $("btn-listen").textContent = "识别鼠标键";
  $("listen-copy").textContent = "点击识别，再按侧键、中键或滚轮。也可以直接在下方选择按键。";
  document.querySelector(".listen-sheet")?.classList.remove("armed");
  await invoke("disarm_listen");
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
  recordBuf = [];
  showRecorder();
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      slot === "hold" ? "录长按组合键" : slot === "tap" ? "录短按组合键" : "录切换组合键";
  }
  renderKeys($("record-keys"), [], { dimEmpty: false, removable: true });
  await startRecorder();
}

async function openRecordForNew(button: string, slot?: "tap" | "hold" | "toggle") {
  currentDraft = {
    button,
    isNew: true,
    initialKeys: [],
    slot,
  };
  recordBuf = [];
  showRecorder();
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      `设置${BUTTON_LABEL[button] ?? button}的快捷键`;
  }
  renderKeys($("record-keys"), [], { removable: true });
  await startRecorder();
}

async function closeRecord() {
  currentDraft = null;
  recordBuf = [];
  $("record-mask").classList.add("hidden");
  document.querySelector<HTMLElement>(".app")!.inert = false;
  if (focusBeforeRecord?.isConnected) focusBeforeRecord.focus();
  focusBeforeRecord = null;
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
  const draft = currentDraft;
  let keys: string[] = [];
  try {
    keys = await invoke<string[]>("take_record_keys");
  } catch (err) {
    console.error("take_record_keys error:", err);
  }
  if (currentDraft !== draft) return;
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
  await closeRecord();
}

async function checkHook(attempt = 0): Promise<void> {
  const status = await invoke<string>("get_hook_status");
  if (status === "starting" && attempt < 10) {
    window.setTimeout(() => { void checkHook(attempt + 1); }, 500);
    return;
  }
  hookReady = status === "ready";
  if (hookReady) setPausedUi(isPaused);
  else {
    $("engine-status").textContent = "监听需要处理";
    showAlert(status === "starting" ? "监听启动超时，请重新启动应用。" : status);
  }
}

async function boot() {
  await listen("mappings-changed", async () => {
    await saveQueue;
    const latest = await invoke<Snapshot>("get_snapshot");
    mappings = sanitizeMappings(latest.config.mappings);
    confirmedMappings = structuredClone(mappings);
    renderMaps();
    $("save-status").textContent = "已保存 · 即时生效";
  });
  const snap = await invoke<Snapshot>("get_snapshot");
  const rawMappings = snap.config.mappings ?? [];
  mappings = sanitizeMappings(rawMappings);
  confirmedMappings = structuredClone(mappings);

  if (mappings.length) {
    selectedMappingId = mappings[0].id;
  }
  if (JSON.stringify(rawMappings) !== JSON.stringify(mappings)) {
    await persist();
  }

  applyTheme(snap.config.theme || "light");
  setPausedUi(!!snap.config.paused);
  void checkHook();
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

  if (isMac) {
    $("safety-copy").textContent = "左键与右键始终保留原操作。首次使用请授予辅助功能权限；异常时可从托盘暂停。";
    document.querySelector<HTMLButtonElement>('[data-key="LWin"]')!.textContent = "⌘ Command";
  }
  paintDots("");
  renderMaps();

  if (snap.last) {
    $("pulse-name").textContent = BUTTON_LABEL[snap.last.button] ?? snap.last.button;
    highlightMouse(snap.last.button);
    paintDots(snap.last.button);
  }

  const pendingPulses = new Map<string, Pulse>();
  let pendingDown: Pulse | null = null;
  let feedbackTimer = 0;
  let pulseRaf = 0;
  const applyPulse = (p: Pulse) => {
    $("pulse-name").textContent = BUTTON_LABEL[p.button] ?? p.button;
    $("pulse-state").textContent = p.down
      ? p.swallowed
        ? "已拦截，改发快捷键"
        : "保留系统原操作"
      : "松开";
    $("pulse-ago").textContent = p.down ? "按下" : "松开";
    highlightMouse(p.down ? p.button : "");
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
    pendingPulses.set(ev.payload.button, ev.payload);
    if (ev.payload.down) pendingDown = ev.payload;
    if (pulseRaf) return;
    pulseRaf = window.requestAnimationFrame(() => {
      pulseRaf = 0;
      for (const pulse of pendingPulses.values()) applyPulse(pulse);
      pendingPulses.clear();
      if (pendingDown) {
        const pulse = pendingDown;
        const host = document.querySelector<HTMLElement>(".pulse-sheet")!;
        if (!reducedMotion.matches) {
          host.getAnimations().forEach((animation) => animation.cancel());
          const ring = document.querySelector<HTMLElement>(".signal-ring")!;
          ring.getAnimations().forEach((animation) => animation.cancel());
          ring.animate([{ transform: "scale(.7)", opacity: "var(--impact-opacity)" }, { transform: "scale(1.55)", opacity: 0 }], { duration: 480, easing: "cubic-bezier(.16,1,.3,1)" });
          host.animate([{ transform: "scale(1)" }, { transform: "scale(1.012)" }, { transform: "scale(1)" }], { duration: 320, easing: "cubic-bezier(.2,.8,.2,1)" });
        }
        highlightMouse(pulse.button);
        clearTimeout(feedbackTimer);
        feedbackTimer = window.setTimeout(() => {
          highlightMouse("");
          document.querySelectorAll(".map.live").forEach((el) => el.classList.remove("live"));
        }, 240);
        pendingDown = null;
      }
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
    await stopListening();
    if (currentDraft) return;

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
      await openRecordForNew(capturedButton);
    }
  });

  await listen<string[]>("record-keys", (ev) => {
    if (!currentDraft) return;
    recordBuf = ev.payload ?? [];
    renderKeys($("record-keys"), recordBuf, { removable: true });
  });

  await listen("record-cancel", () => {
    closeRecord();
  });
}

$("btn-theme").addEventListener("click", async () => {
  const button = $("btn-theme") as HTMLButtonElement;
  const previous = document.documentElement.dataset.theme || "light";
  const next = previous === "dark" ? "light" : "dark";
  button.disabled = true;
  applyTheme(next);
  try { await invoke("save_theme", { theme: next }); }
  catch (err) { applyTheme(previous); showAlert(`主题保存失败：${String(err)}`); }
  finally { button.disabled = false; }
});

$("btn-pause").addEventListener("click", async () => {
  const next = !isPaused;
  const button = $("btn-pause") as HTMLButtonElement;
  button.disabled = true;
  try {
    await invoke("save_paused", { paused: next });
    setPausedUi(next);
  } catch (err) { showAlert(`暂停状态保存失败：${String(err)}`); }
  finally { button.disabled = false; }
});

$("btn-listen").addEventListener("click", async () => {
  if (listening) { await stopListening(); return; }
  listening = true;
  $("listen-copy").textContent = "请按侧键、中键或滚轮。15 秒后自动结束识别。";
  $("btn-listen").textContent = "取消识别";
  document.querySelector(".listen-sheet")?.classList.add("armed");
  try {
    await invoke("arm_listen");
    listenTimer = window.setTimeout(() => { void stopListening(); }, 15000);
  } catch (err) {
    await stopListening();
    showAlert(`无法识别：${String(err)}`);
  }
});

$("quick-add").addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLElement>("[data-add-button]")?.dataset.addButton;
  if (!button || currentDraft) return;
  const existing = mappings.find(m => m.button === button);
  if (existing) {
    selectedMappingId = existing.id;
    renderMaps();
    return;
  }
  await stopListening();
  await openRecordForNew(button);
});

$("record-presets").addEventListener("click", async (event) => {
  const preset = (event.target as HTMLElement).closest<HTMLElement>("[data-preset]")?.dataset.preset;
  if (!preset || !currentDraft) return;
  const keys: Record<string, string[]> = {
    copy: [isMac ? "LWin" : "LControl", "C"],
    paste: [isMac ? "LWin" : "LControl", "V"],
    undo: [isMac ? "LWin" : "LControl", "Z"],
    enter: ["Enter"],
  };
  if (!keys[preset]) return;
  await invoke("arm_record");
  for (const key of keys[preset]) await invoke("add_record_key", { key });
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
  if (!sel.matches("select[data-k]")) return;

  if (sel.dataset.k === "button") {
    const nextBtn = sel.value;
    const isConflict = mappings.some((other) => other.id !== m.id && other.button === nextBtn);
    if (isConflict) {
      sel.value = m.button;
      return;
    }
    runtimeStates.delete(m.button);
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
      m.keys = m.tap_keys?.length ? m.tap_keys : m.hold_keys?.length ? m.hold_keys : m.keys;
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
});

$("maps").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const row = t.closest(".map") as HTMLElement | null;
  if (!row) return;
  const id = row.dataset.id!;

  if (t.dataset.act === "collapse") {
    if (collapsedMappings.has(id)) collapsedMappings.delete(id); else collapsedMappings.add(id);
    row.classList.toggle("collapsed", collapsedMappings.has(id));
    t.textContent = collapsedMappings.has(id) ? "展开" : "折叠";
    t.setAttribute("aria-expanded", String(!collapsedMappings.has(id)));
    return;
  }
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
      if (slot !== "toggle") target.keys = target.tap_keys?.length ? [...target.tap_keys] : [...(target.hold_keys ?? [])];
      await persist();
    }
    return;
  }

  if (t.dataset.act === "record") {
    const slot = (t.dataset.slot as "tap" | "hold" | "toggle") || "tap";
    selectMapping(id);
    await openRecordForExisting(id, slot);
    return;
  }

  if (selectedMappingId !== id) {
    selectMapping(id);
  }
});

$("record-cancel").addEventListener("click", () => {
  closeRecord();
});

$("record-ok").addEventListener("click", async () => {
  if (confirming) return;
  confirming = true;
  ($( "record-ok") as HTMLButtonElement).disabled = true;
  try { await confirmRecord(); }
  finally { confirming = false; ($("record-ok") as HTMLButtonElement).disabled = false; }
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
    if (e.key === "Tab") {
      const buttons = Array.from($("record-mask").querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(index + (e.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
      e.preventDefault();
      return;
    }
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

async function checkForUpdate(current: string) {
  const status = $("update-status");
  const button = $("btn-check-update") as HTMLButtonElement;
  if (button.disabled) return;
  button.disabled = true;
  status.textContent = "正在检查稳定版…";
  try {
    const release = await latestRelease();
    status.textContent = isNewer(release.version, current) ? `可更新至 ${release.version}` : "当前已是最新版本";
    if (isNewer(release.version, current)) {
      const link = document.createElement("button");
      link.className = "ghost";
      link.textContent = "查看发行说明";
      link.addEventListener("click", () => { void invoke("open_url", { url: release.url }); });
      status.appendChild(link);
    }
  } catch {
    status.textContent = "暂时无法连接 GitHub，请稍后重试。";
    const link = document.createElement("button");
    link.className = "ghost";
    link.textContent = "打开下载页面";
    link.addEventListener("click", () => { void invoke("open_url", { url: RELEASES_URL }); });
    status.appendChild(link);
  } finally {
    button.disabled = false;
  }
}

window.addEventListener("blur", () => {
  if (currentDraft) void closeRecord();
  if (listening) void stopListening();
});
window.addEventListener("unhandledrejection", (event) => {
  showAlert(`操作未完成：${String(event.reason)}`);
});

boot()
  .then(async () => {
    try {
      const ver = await invoke<string>("app_version");
      $("app-version").textContent = `v${ver}`;
      $("btn-check-update").addEventListener("click", () => {
        void checkForUpdate(ver);
      });
    } catch {
      $("app-version").textContent = "版本读取失败";
    }
  })
  .catch((err) => {
    showAlert(`无法连接映射引擎，请重新打开控制面板：${String(err)}`);
  });

type CardDrag = { x: number; lastX: number; lastTime: number; velocity: number; dx: number; id: number; grip: HTMLElement; card: HTMLElement };
let drag: CardDrag | null = null;
let deckBusy = false;
let deckEpoch = 0;
let dragFrame = 0;
const deckAnimations = new Set<Animation>();
let preview: HTMLElement | null = null;
let previewTarget: HTMLElement | null = null;

function resetDeck() {
  ++deckEpoch;
  const previous = drag;
  drag = null;
  if (previous?.grip.hasPointerCapture(previous.id)) previous.grip.releasePointerCapture(previous.id);
  cancelAnimationFrame(dragFrame);
  dragFrame = 0;
  for (const animation of deckAnimations) animation.cancel();
  deckAnimations.clear();
  document.querySelectorAll<HTMLElement>("#maps > .map").forEach(card => {
    card.style.removeProperty("transform");
    card.style.removeProperty("filter");
    card.style.removeProperty("opacity");
  });
  preview?.remove();
  preview = previewTarget = null;
  $("maps").style.removeProperty("min-height");
  $("maps").classList.remove("deck-moving");
  deckBusy = false;
}
function nextCard(direction: number): HTMLElement | null {
  const cards = [...$("maps").querySelectorAll<HTMLElement>(":scope > .map")];
  return cards.length < 2 ? null : direction > 0 ? cards[1] : cards[cards.length - 1];
}
function preparePreview(direction: number) {
  const target = nextCard(direction);
  if (!target || previewTarget === target) return;
  preview?.remove();
  previewTarget = target;
  preview = target.cloneNode(true) as HTMLElement;
  preview.className = `deck-preview${target.classList.contains("collapsed") ? " collapsed" : ""}`;
  preview.removeAttribute("data-id");
  preview.removeAttribute("data-button");
  preview.inert = true;
  preview.setAttribute("aria-hidden", "true");
  $("maps").append(preview);
}
function paintDrag() {
  dragFrame = 0;
  if (!drag) return;
  preparePreview(drag.dx >= 0 ? 1 : -1);
  const width = $("maps").clientWidth;
  const dx = Math.max(-width, Math.min(width, drag.dx));
  const progress = Math.min(1, Math.abs(dx) / width);
  drag.card.style.transform = `translateX(${dx}px) rotate(${dx / width * 3}deg)`;
  drag.card.style.filter = reducedMotion.matches ? "none" : `blur(${progress * 2}px)`;
  if (preview) {
    preview.style.transform = `translateY(${12 * (1 - progress)}px) scale(${.96 + .04 * progress})`;
    preview.style.filter = reducedMotion.matches ? "none" : `blur(${4 * (1 - progress)}px)`;
    preview.style.opacity = String(.5 + .5 * progress);
  }
}
async function animateDeck(el: HTMLElement, frames: Keyframe[], duration: number) {
  if (reducedMotion.matches) return;
  const animation = el.animate(frames, { duration, easing: "cubic-bezier(.22,.75,.25,1)", fill: "forwards" });
  deckAnimations.add(animation);
  try { await animation.finished; } catch { /* Cancelled by blur, capture loss or a rerender. */ }
}
async function cycleCard(direction: number, fromDrag = false) {
  if (deckBusy || currentDraft) return;
  const host = $("maps");
  const first = host.querySelector<HTMLElement>(":scope > .map");
  const next = nextCard(direction);
  if (!first || !next) { resetDeck(); return; }
  deckBusy = true;
  const epoch = deckEpoch;
  host.classList.add("deck-moving");
  host.style.minHeight = `${host.offsetHeight}px`;
  preparePreview(direction);
  const transform = fromDrag ? first.style.transform : "none";
  await Promise.all([
    animateDeck(first, [
      { transform, filter: first.style.filter || "blur(0px)", opacity: 1 },
      { transform: `translateX(${direction * (host.clientWidth + 40)}px) rotate(${direction * 3}deg)`, filter: "blur(4px)", opacity: 0 }
    ], 260),
    ...(preview ? [animateDeck(preview, [
      { transform: preview.style.transform || "translateY(12px) scale(.96)", filter: preview.style.filter || "blur(4px)", opacity: preview.style.opacity || .5 },
      { transform: "none", filter: "blur(0px)", opacity: 1 }
    ], 260)] : [])
  ]);
  if (epoch !== deckEpoch) return;
  resetDeck();
  if (direction > 0) host.append(first); else host.prepend(next);
  selectMapping(next.dataset.id!);
}
$("deck-prev").onclick = () => { void cycleCard(-1); };
$("deck-next").onclick = () => { void cycleCard(1); };
$("maps").addEventListener("pointerdown", e => {
  const grip = (e.target as HTMLElement).closest<HTMLElement>(".card-grip");
  if (!grip || e.button !== 0 || !e.isPrimary || deckBusy || mappings.length < 2) return;
  resetDeck();
  const card = grip.closest<HTMLElement>(".map")!;
  drag = { x: e.clientX, lastX: e.clientX, lastTime: e.timeStamp, velocity: 0, dx: 0, id: e.pointerId, grip, card };
  grip.setPointerCapture(e.pointerId);
  $("maps").classList.add("deck-moving");
});
$("maps").addEventListener("pointermove", e => {
  if (!drag || e.pointerId !== drag.id) return;
  if (!(e.buttons & 1)) { resetDeck(); return; }
  const elapsed = e.timeStamp - drag.lastTime;
  if (elapsed > 0) drag.velocity = (e.clientX - drag.lastX) / elapsed;
  drag.lastX = e.clientX;
  drag.lastTime = e.timeStamp;
  drag.dx = e.clientX - drag.x;
  if (!dragFrame) dragFrame = requestAnimationFrame(paintDrag);
});
$("maps").addEventListener("pointerup", e => {
  if (!drag || e.pointerId !== drag.id) return;
  cancelAnimationFrame(dragFrame);
  drag.dx = e.clientX - drag.x;
  paintDrag();
  const finished = drag;
  drag = null; // Clear before releasing capture: lostpointercapture must not cancel the settle.
  if (finished.grip.hasPointerCapture(e.pointerId)) finished.grip.releasePointerCapture(e.pointerId);
  const distance = Math.abs(finished.dx);
  const flick = e.timeStamp - finished.lastTime < 100 && Math.abs(finished.velocity) > .5 && distance > 25;
  if (distance >= Math.min(110, $("maps").clientWidth * .18) || flick) {
    void cycleCard(finished.dx >= 0 ? 1 : -1, true);
  } else {
    const epoch = deckEpoch;
    deckBusy = true;
    void animateDeck(finished.card, [
      { transform: finished.card.style.transform, filter: finished.card.style.filter },
      { transform: "none", filter: "blur(0px)" }
    ], 180).then(() => { if (epoch === deckEpoch) resetDeck(); });
  }
});
$("maps").addEventListener("pointercancel", resetDeck);
$("maps").addEventListener("lostpointercapture", () => { if (drag) resetDeck(); });
window.addEventListener("blur", resetDeck);
window.addEventListener("keydown", () => { if (drag || deckBusy) resetDeck(); });
window.addEventListener("resize", resetDeck);
document.addEventListener("visibilitychange", () => { if (document.hidden) resetDeck(); });
reducedMotion.addEventListener("change", resetDeck);
document.querySelectorAll<HTMLAnchorElement>(".rail-link").forEach(link => {
  link.addEventListener("click", () => {
    document.querySelectorAll(".rail-link").forEach(el => el.classList.toggle("active", el === link));
  });
});
