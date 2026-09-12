import { isNewer, latestRelease, RELEASES_URL } from "./updates";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  applyButtonChange,
  applyModeChange,
  removeKeyFromSlot,
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
  emergency_hotkeys: number;
  active_bindings: { mapping_id: string; button: string; mode: string; active: boolean }[];
  recovery_notes: string[];
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
let keysFromBackend = false;
let isPaused = false;
const isMac = /Mac/i.test(
  (navigator as any).userAgentData?.platform ?? navigator.platform
);
let listenGen = 0;
let localGen = 0;
let draftGen = 0;
let hookReady = false;
let listenTimer = 0;
let confirmedMappings: Mapping[] = [];
let saveQueue: Promise<void> = Promise.resolve();
let saveInFlight = 0;
let pendingRemote: Mapping[] | null = null;
let saveRevision = 0;
let confirming = false;
let focusBeforeRecord: HTMLElement | null = null;
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
const runtimeStates: Map<string, RuntimeState> = new Map();
const pendingPulses = new Map<string, Pulse>();
let pendingDown: Pulse | null = null;
// Wheel pulses behave like a held button: same-direction ticks only refresh
// wheelHoldTimer; the zone/ring UI is not re-triggered until it expires.
let wheelHoldButton: string | null = null;
let wheelHoldTimer = 0;
let scrollSpyStarted = false;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function invokeT<T>(cmd: string, args?: Record<string, unknown>, ms = 8000): Promise<T> {
  return Promise.race([
    invoke<T>(cmd, args),
    new Promise<never>((_, reject) =>
      window.setTimeout(() => reject(new Error(`操作超时（${cmd}）`)), ms)
    ),
  ]);
}

async function safeInvoke(
  cmd: string,
  args?: Record<string, unknown>,
  failMsg?: string
): Promise<void> {
  try {
    await invokeT(cmd, args);
  } catch (err) {
    showAlert(failMsg ?? `操作未完成：${String(err)}`);
  }
}

let alertTimer = 0;
let currentAlertKind: string | null = null;

function showAlert(
  msg: string,
  opts: {
    sticky?: boolean;
    timeout?: number;
    kind?: "save" | "hook" | "recovery" | "undo" | "info";
    action?: { label: string; onClick: () => void };
  } = {}
) {
  const banner = $("alert-banner");
  if (!banner) return;
  clearTimeout(alertTimer);
  currentAlertKind = opts.kind ?? "info";
  $("alert-text").textContent = msg;
  const action = $("alert-action") as HTMLButtonElement;
  if (opts.action) {
    action.hidden = false;
    action.textContent = opts.action.label;
    action.onclick = () => opts.action?.onClick();
  } else {
    action.hidden = true;
    action.onclick = null;
  }
  banner.classList.remove("hidden");
  if (!opts.sticky) {
    alertTimer = window.setTimeout(() => hideAlert(), opts.timeout ?? 6000);
  }
}

function hideAlert(kind?: string) {
  if (kind !== undefined && kind !== currentAlertKind) return;
  clearTimeout(alertTimer);
  currentAlertKind = null;
  $("alert-banner")?.classList.add("hidden");
}

function prettyKeys(keys: string[]): string[] {
  if (!keys.length) return ["空"];
  const map: Record<string, string> = isMac
    ? {
        LControl: "⌃ Control", RControl: "⌃ Right Control",
        LAlt: "⌥ Option", RAlt: "⌥ Right Option",
        LShift: "⇧ Shift", RShift: "⇧ Right Shift",
        LWin: "⌘ Command", RWin: "⌘ Right Command",
      }
    : {
        LControl: "Left Ctrl", RControl: "Right Ctrl",
        LAlt: "Left Alt", RAlt: "Right Alt",
        LShift: "Left Shift", RShift: "Right Shift",
        LWin: "Left Win", RWin: "Right Win",
      };
  const common: Record<string, string> = {
    Space: "空格", Enter: "回车", Backspace: "退格", Escape: "Esc",
    Delete: "Delete", Insert: "Insert", Home: "Home", End: "End",
    PageUp: "PgUp", PageDown: "PgDn", Tab: "Tab",
    ArrowLeft: "←", ArrowRight: "→", ArrowUp: "↑", ArrowDown: "↓",
  };
  return keys.map((k) => map[k] ?? common[k] ?? k);
}

function renderKeys(
  el: HTMLElement,
  keys: string[],
  opts: { dimEmpty?: boolean; removable?: boolean; emptyText?: string } = {}
) {
  const dimEmpty = opts.dimEmpty !== false;
  const removable = !!opts.removable;
  el.replaceChildren();
  if (!keys.length) {
    const span = document.createElement("span");
    span.className = `key${dimEmpty ? " dim" : ""}`;
    span.textContent = opts.emptyText ?? "空";
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

function flashPauseGlyph(paused: boolean) {
  if (reducedMotion.matches || document.hidden) return;
  const btnRect = $("btn-pause").getBoundingClientRect();
  const anchorX = `${btnRect.left + btnRect.width / 2}px`;
  const anchorY = `${btnRect.top + btnRect.height / 2}px`;
  for (let i = 0; i < 2; i++) {
    const el = document.createElement("div");
    el.className = `pause-flash ${paused ? "is-pause" : "is-resume"}`;
    el.setAttribute("aria-hidden", "true");
    // Anchor to #btn-pause center; kill the old inset:0 + margin:auto centering.
    el.style.inset = "auto";
    el.style.left = anchorX;
    el.style.top = anchorY;
    el.style.margin = "0";
    el.style.transform = "translate(-50%,-50%)";
    el.innerHTML = paused
      ? '<svg viewBox="0 0 64 64" fill="currentColor"><rect x="18" y="12" width="10" height="40" rx="4"/><rect x="36" y="12" width="10" height="40" rx="4"/></svg>'
      : '<svg viewBox="0 0 64 64" fill="currentColor" stroke="currentColor" stroke-width="7" stroke-linejoin="round"><path d="M20 12 L50 32 L20 52 Z"/></svg>';
    document.body.appendChild(el);
    const animation = el.animate(
      [
        { opacity: 0, transform: "translate(-50%,-50%) scale(.85)", filter: "blur(0px)" },
        { opacity: 1, transform: "translate(-50%,-50%) scale(1)", filter: "blur(0px)", offset: 0.12 },
        { opacity: 0, transform: "translate(-50%,-50%) scale(1.9)", filter: "blur(18px)" },
      ],
      { duration: 660, delay: i * 60, easing: "cubic-bezier(.22,.8,.3,1)", fill: "backwards" }
    );
    animation.finished.catch(() => {}).finally(() => el.remove());
  }
}

function setPausedUi(paused: boolean) {
  const changed = paused !== isPaused;
  isPaused = paused;
  if (hookReady) {
    $("engine-status").textContent = mappings.length === 0
      ? "尚未配置映射"
      : paused ? "映射已暂停" : "映射已启用";
  }
  $("engine-status").classList.toggle("paused", paused);
  $("btn-pause").setAttribute("aria-pressed", String(paused));
  $("btn-pause").textContent = paused ? "恢复映射" : "暂停映射";
  if (paused) {
    runtimeStates.clear();
    updateAllRuntimePills();
  }
  if (changed) flashPauseGlyph(paused);
}

/** Write text into a meta element; play a small bump animation only when the value changed. */
function bumpText(el: HTMLElement, text: string) {
  if (el.textContent === text) return;
  el.textContent = text;
  if (!reducedMotion.matches) {
    el.animate(
      [{ transform: "translateY(4px)", opacity: 0.4 }, { transform: "none", opacity: 1 }],
      { duration: 180, easing: "ease-out" }
    );
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
  } else if ((m.mode === "dual" || !m.mode) && (m.hold_keys?.length ?? 0)) {
    return isActive && state?.mode === "hold"
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
    return;
  }
  const mode = selected.button.startsWith("wheel") ? "click" : selected.mode === "toggle" ? "toggle" : "dual";
  $("dock-lead").textContent = `${BUTTON_LABEL[selected.button] ?? selected.button} · ${MODE_LABEL[mode] ?? mode}`;
  const shown =
    selected.mode === "toggle"
      ? selected.keys
      : (selected.tap_keys?.length ? selected.tap_keys : selected.hold_keys) ?? selected.keys;
  renderKeys($("dock-keys"), shown ?? []);
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
  setPausedUi(isPaused);
  resetDeck();
  const host = $("maps");
  bumpText($("mapping-count"), `${mappings.length} / 5 已配置`);
  document.querySelectorAll<HTMLButtonElement>("[data-add-button]").forEach((button) => {
    const existing = mappings.find((m) => m.button === button.dataset.addButton);
    button.textContent = `${existing ? "" : "＋ "}${BUTTON_LABEL[button.dataset.addButton!]}`;
    button.classList.toggle("active", existing?.id === selectedMappingId);
    button.setAttribute("aria-pressed", String(existing?.id === selectedMappingId));
  });
  if (!mappings.length) {
    host.replaceChildren();
    const p = document.createElement("p");
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
  updateDeckIndex();

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
      opt.textContent = isOccupied ? `${labelText}（已绑定）` : labelText;
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
    selMode.disabled = availableModes.length <= 1;
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
    const gripText = document.createElement("span");
    gripText.textContent = `${BUTTON_LABEL[m.button] ?? m.button} `;
    const gripHint = document.createElement("span");
    gripHint.className = "grip-hint";
    gripHint.textContent = "· 拖动切换";
    gripText.appendChild(gripHint);
    grip.appendChild(gripText);
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
    const hasAnyKeys =
      (m.keys?.length ?? 0) + (m.tap_keys?.length ?? 0) + (m.hold_keys?.length ?? 0) > 0;
    desc.textContent = hasAnyKeys
      ? MODE_DESC[uiMode] ?? MODE_DESC[m.mode] ?? ""
      : "此映射未绑定任何键，不会生效";
    article.append(desc);
    host.appendChild(article);
  });

  renderDock();
}

function updateDeckIndex() {
  const el = $("deck-index");
  if (!el) return;
  const idx = mappings.findIndex((m) => m.id === selectedMappingId);
  bumpText(el, mappings.length && idx >= 0 ? `第 ${idx + 1} / ${mappings.length} 张` : "");
}

function applyRemoteMappings(remote: Mapping[]) {
  mappings = remote;
  confirmedMappings = structuredClone(mappings);
  if (!mappings.some((m) => m.id === selectedMappingId)) {
    selectedMappingId = mappings[0]?.id ?? null;
  }
  renderMaps();
  $("save-status").textContent = "已保存 · 即时生效";
}

async function persist() {
  mappings = sanitizeMappings(mappings);
  const snapshot = structuredClone(mappings);
  const revision = ++saveRevision;
  renderMaps();
  $("save-status").textContent = "正在保存…";
  saveInFlight++;
  saveQueue = saveQueue
    .then(async () => {
      try {
        await invokeT("save_mappings", { mappings: snapshot });
        confirmedMappings = snapshot;
        hideAlert("save");
        if (revision === saveRevision) $("save-status").textContent = "已保存 · 即时生效";
      } catch (err) {
        if (revision === saveRevision) {
          mappings = structuredClone(confirmedMappings);
          renderMaps();
          $("save-status").textContent = "保存失败 · 已恢复";
        }
        if (err instanceof Error && err.message.startsWith("操作超时")) {
          saveQueue = Promise.resolve();
          showAlert("保存超时，映射引擎可能卡住。已恢复到上次保存的配置。", { sticky: true, kind: "save" });
        } else {
          showAlert(`保存配置失败：${String(err)}`, { kind: "save" });
        }
      } finally {
        saveInFlight--;
        if (saveInFlight === 0 && pendingRemote) {
          const remote = pendingRemote;
          pendingRemote = null;
          applyRemoteMappings(remote);
        }
      }
    })
    .catch(() => {});
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
  updateDeckIndex();
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
    await invokeT("arm_record");
  } catch (err) {
    await closeRecord();
    showAlert(`无法开始录制：${String(err)}`);
  }
}

async function stopListening() {
  listenGen = 0;
  clearTimeout(listenTimer);
  $("btn-listen").textContent = "识别鼠标键";
  $("listen-copy").textContent = "点击识别，再按侧键、中键或滚轮。也可以直接在下方选择按键。";
  document.querySelector(".listen-sheet")?.classList.remove("armed");
  await safeInvoke("disarm_listen");
}

async function openRecordForExisting(id: string, slot: "tap" | "hold" | "toggle" = "tap") {
  const m = mappings.find((x) => x.id === id);
  if (!m) return;
  currentDraft = {
    button: m.button,
    isNew: false,
    existingId: m.id,
    slot,
    gen: ++draftGen,
  };
  recordBuf = [];
  keysFromBackend = false;
  showRecorder();
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      slot === "hold" ? "录长按组合键" : slot === "tap" ? "录短按组合键" : "录切换组合键";
  }
  renderKeys($("record-keys"), [], { dimEmpty: false, removable: true, emptyText: "正在监听键盘…" });
  updateRecordOk();
  await startRecorder();
}

async function openRecordForNew(button: string, slot?: "tap" | "hold" | "toggle") {
  currentDraft = {
    button,
    isNew: true,
    slot,
    gen: ++draftGen,
  };
  recordBuf = [];
  keysFromBackend = false;
  showRecorder();
  const title = $("record-mask").querySelector("h2");
  if (title) {
    title.textContent =
      button === "wheelup" || button === "wheeldown"
        ? "检测到滚轮 · 设置滚动快捷键"
        : `为「${BUTTON_LABEL[button] ?? button}」设置快捷键`;
  }
  renderKeys($("record-keys"), [], { removable: true, emptyText: "正在监听键盘…" });
  updateRecordOk();
  await startRecorder();
}

const RECORD_HELP_DEFAULT =
  "按下键盘组合，或选择上方预设。Esc 取消，Tab 切换焦点；如需映射它们，请点击对应按键。录制期间暂停触发鼠标映射。";

function updateRecordOk() {
  const ok = $("record-ok") as HTMLButtonElement;
  const help = $("record-help");
  if (recordBuf.length >= 16) {
    ok.disabled = true;
    if (help) help.textContent = "组合键最多 16 键";
    return;
  }
  if (help) help.textContent = RECORD_HELP_DEFAULT;
  ok.disabled = confirming || (recordBuf.length === 0 && !keysFromBackend);
}

async function closeRecord() {
  currentDraft = null;
  recordBuf = [];
  keysFromBackend = false;
  confirming = false;
  ($("record-ok") as HTMLButtonElement).disabled = false;
  $("record-mask").classList.add("hidden");
  document.querySelector<HTMLElement>(".app")!.inert = false;
  if (focusBeforeRecord?.isConnected) focusBeforeRecord.focus();
  focusBeforeRecord = null;
  try {
    await invokeT("disarm_record");
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
  if (currentDraft.gen !== draft.gen) return;
  let keys: string[] = [];
  try {
    keys = await invokeT<string[]>("take_record_keys");
  } catch (err) {
    console.error("take_record_keys error:", err);
  }
  if (currentDraft?.gen !== draft.gen) return;
  const rawKeys = keys.length ? keys : recordBuf;
  const finalKeys = normalizeKeyChord(rawKeys);
  if (!finalKeys.length) {
    showAlert("未录入任何按键，录制已取消。");
    await closeRecord();
    return;
  }

  const slot = draft.slot ?? (inferTriggerMode(draft.button, finalKeys) === "hold" ? "hold" : "tap");
  if (draft.isNew) {
    const newMapping: Mapping = normalizeMapping({
      id: crypto.randomUUID(),
      button: draft.button,
      mode: slot === "toggle" ? "toggle" : "dual",
      keys: slot === "toggle" ? [...finalKeys] : [],
      tap_keys: slot === "tap" ? [...finalKeys] : [],
      hold_keys: slot === "hold" ? [...finalKeys] : [],
    });
    mappings.push(newMapping);
    selectedMappingId = newMapping.id;
  } else {
    const existing = mappings.find((x) => x.id === draft.existingId);
    if (existing) {
      if (slot === "toggle") {
        existing.mode = "toggle";
        existing.keys = [...finalKeys];
      } else if (slot === "hold") {
        existing.hold_keys = [...finalKeys];
        existing.mode = "dual";
      } else {
        existing.tap_keys = [...finalKeys];
        if (existing.mode !== "toggle") existing.mode = "dual";
      }
      mappings = mappings.map((x) => (x.id === existing.id ? normalizeMapping(existing) : x));
    }
  }

  if (currentDraft?.gen !== draft.gen) return;
  await persist();
  if (currentDraft?.gen !== draft.gen) return;
  hideAlert("save");
  await closeRecord();
}

function applyHookStatus(status: string) {
  if (status === "starting") {
    $("engine-status").textContent = "正在检查监听";
    return;
  }
  if (status === "ready") {
    hookReady = true;
    setPausedUi(isPaused);
    hideAlert("hook");
    return;
  }
  hookReady = false;
  $("engine-status").textContent = "监听需要处理";
  if (isMac && status.includes("辅助功能")) {
    showAlert(status, {
      sticky: true,
      kind: "hook",
      action: {
        label: "打开辅助功能设置",
        onClick: () => {
          void safeInvoke("open_accessibility_settings");
          window.setTimeout(() => void safeInvoke("retry_hook"), 3000);
        },
      },
    });
    return;
  }
  showAlert(status, {
    sticky: true,
    kind: "hook",
    action: { label: "重试监听", onClick: () => void safeInvoke("retry_hook") },
  });
}

async function checkHook(attempt = 0): Promise<void> {
  try {
    const status = await invokeT<string>("get_hook_status");
    if (status === "starting" && attempt < 10) {
      window.setTimeout(() => { void checkHook(attempt + 1); }, 500);
      return;
    }
    applyHookStatus(status === "starting" ? "监听启动超时，请重新启动应用。" : status);
  } catch (err) {
    $("engine-status").textContent = "无法读取监听状态";
    showAlert(`无法读取监听状态：${String(err)}`, { kind: "hook" });
  }
}

async function boot() {
  const unlisteners: UnlistenFn[] = [];

  unlisteners.push(
    await listen<Mapping[]>("mappings-changed", (ev) => {
      const remote = sanitizeMappings(ev.payload ?? []);
      if (saveInFlight > 0) {
        pendingRemote = remote;
        return;
      }
      applyRemoteMappings(remote);
    })
  );

  let feedbackTimer = 0;
  let pulseRaf = 0;
  const isWheelButton = (button: string) => button === "wheelup" || button === "wheeldown";
  const setPulseText = (id: string, text: string) => {
    const el = $(id);
    if (el.textContent !== text) el.textContent = text;
  };
  const releasePulseUi = () => {
    highlightMouse("");
    document.querySelectorAll(".map.live").forEach((el) => el.classList.remove("live"));
  };
  const clearWheelHold = () => {
    wheelHoldButton = null;
    clearTimeout(wheelHoldTimer);
  };
  const applyPulse = (p: Pulse) => {
    if (listenGen && p.down && (p.button === "left" || p.button === "right")) {
      $("listen-copy").textContent = "左/右键仅用于识别，不支持映射。请按侧键、中键或滚轮。";
    }
    const wheel = isWheelButton(p.button);
    const wheelHeld = wheel && wheelHoldButton === p.button;
    setPulseText("pulse-name", BUTTON_LABEL[p.button] ?? p.button);
    setPulseText("pulse-state", p.down
      ? p.swallowed
        ? "已拦截，改发快捷键"
        : "保留系统原操作"
      : "松开");
    setPulseText("pulse-ago", p.down ? "按下" : "松开");
    // While a same-direction wheel hold is active the zone is already lit —
    // skipping this keeps .zone.on (and its CSS animation) from re-triggering.
    if (!wheelHeld) highlightMouse(wheel || p.down ? p.button : "");
    paintDots(p.button);

    document.querySelectorAll(".map").forEach((el) => {
      el.classList.toggle("live", (el as HTMLElement).dataset.button === p.button && (wheel || p.down));
    });

    if (p.down) {
      const active = mappings.find((m) => m.button === p.button);
      if (active && selectedMappingId !== active.id) {
        selectedMappingId = active.id;
        document.querySelectorAll(".map").forEach((el) => {
          el.classList.toggle("selected", (el as HTMLElement).dataset.id === active.id);
        });
        updateDeckIndex();
        renderDock();
      }
    }

  };
  unlisteners.push(
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
          pendingDown = null;
          const wheel = isWheelButton(pulse.button);
          const continuingWheel = wheel && wheelHoldButton === pulse.button;
          if (wheel) {
            wheelHoldButton = pulse.button;
            clearTimeout(feedbackTimer); // a pending release must not hide the held zone
            clearTimeout(wheelHoldTimer);
            wheelHoldTimer = window.setTimeout(() => {
              wheelHoldButton = null;
              releasePulseUi();
            }, 250);
          } else {
            // A real button took over the highlight; cancel the wheel hold so
            // its expiry can't wipe this button's zone a few frames later.
            clearWheelHold();
          }
          if (!continuingWheel) {
            const host = document.querySelector<HTMLElement>(".pulse-sheet")!;
            if (!reducedMotion.matches) {
              host.getAnimations().forEach((animation) => animation.cancel());
              const ring = document.querySelector<HTMLElement>(".signal-ring")!;
              ring.getAnimations().forEach((animation) => animation.cancel());
              ring.animate([{ transform: "scale(.7)", opacity: "var(--impact-opacity)" }, { transform: "scale(1.55)", opacity: 0 }], { duration: 480, easing: "cubic-bezier(.16,1,.3,1)" });
              host.animate([{ transform: "scale(1)" }, { transform: "scale(1.012)" }, { transform: "scale(1)" }], { duration: 320, easing: "cubic-bezier(.2,.8,.2,1)" });
            }
            highlightMouse(pulse.button);
          }
          if (!wheel) {
            clearTimeout(feedbackTimer);
            feedbackTimer = window.setTimeout(releasePulseUi, 240);
          }
        }
      });
    })
  );

  unlisteners.push(
    await listen<{ button: string; active: boolean; mode?: string }>(
      "runtime-binding-changed",
      (ev) => {
        const { button, active, mode } = ev.payload;
        runtimeStates.set(button, { active, mode });
        updateAllRuntimePills();
      }
    )
  );

  unlisteners.push(
    await listen<{ paused: boolean }>("engine-state-changed", (ev) => {
      setPausedUi(ev.payload.paused);
    })
  );

  unlisteners.push(
    await listen<SendReport>("injection-error", (ev) => {
      const r = ev.payload;
      if (r.is_uipi_blocked) {
        showAlert("⚠️ 快捷键注入受阻：目标窗口可能以管理员权限运行（受 Windows UIPI 特权保护）。如有需要，请以管理员身份启动 Mouse Insight。");
      } else {
        showAlert(`⚠️ 按键注入失败（成功 ${r.inserted}/${r.expected}，错误码 ${r.win32_error}）`);
      }
    })
  );

  unlisteners.push(
    await listen<string>("engine-fatal", (ev) => {
      showAlert(`${ev.payload}。请重启应用。`, { sticky: true });
      $("engine-status").textContent = "引擎已停止";
    })
  );

  unlisteners.push(
    await listen<string>("hook-status-changed", (ev) => {
      applyHookStatus(ev.payload);
    })
  );

  unlisteners.push(
    await listen<string>("listen-captured", async (ev) => {
      if (!listenGen) return;
      const capturedButton = ev.payload;
      await stopListening();
      if (currentDraft) return;

      // 左右键仅用于识别，不支持映射（后端不会为主键发此事件，防御保留）
      if (capturedButton === "left" || capturedButton === "right") {
        showAlert("左/右键仅用于识别，为防止误锁系统，不支持映射。");
        $("listen-copy").textContent = `听到了：${BUTTON_LABEL[capturedButton] ?? capturedButton}（仅用于识别，不支持映射）。`;
        $("btn-listen").textContent = "再识别一次";
        document.querySelector(".listen-sheet")?.classList.remove("armed");
        return;
      }

      $("listen-copy").textContent = `听到了：${BUTTON_LABEL[capturedButton] ?? capturedButton}。请录键盘。`;
      $("btn-listen").textContent = "再识别一次";
      document.querySelector(".listen-sheet")?.classList.remove("armed");

      const existing = mappings.find((x) => x.button === capturedButton);
      const inferredSlot =
        capturedButton === "wheelup" || capturedButton === "wheeldown" ? "tap" : "hold";
      if (existing) {
        selectedMappingId = existing.id;
        renderMaps(capturedButton);
        showAlert(`「${BUTTON_LABEL[capturedButton] ?? capturedButton}」已有映射，新录的键会覆盖对应槽位。`);
        const slot =
          existing.mode === "toggle" ? "toggle" : inferredSlot === "hold" && !(existing.hold_keys?.length) && (existing.tap_keys?.length) ? "tap" : inferredSlot;
        await openRecordForExisting(existing.id, slot);
      } else {
        await openRecordForNew(capturedButton);
      }
    })
  );

  unlisteners.push(
    await listen<string[]>("record-keys", (ev) => {
      if (!currentDraft) return;
      recordBuf = ev.payload ?? [];
      if (recordBuf.length) keysFromBackend = true;
      renderKeys($("record-keys"), recordBuf, { removable: true });
      updateRecordOk();
    })
  );

  unlisteners.push(
    await listen("record-cancel", () => {
      closeRecord();
    })
  );

  window.addEventListener("beforeunload", () => {
    for (const u of unlisteners) u();
  });

  const snap = await invokeT<Snapshot>("get_snapshot");
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
    const { isEnabled, enable } = await import("@tauri-apps/plugin-autostart");
    const enabled = await isEnabled();
    autostartBox.checked = enabled;
    if (enabled !== !!snap.config.autostart) {
      await invokeT("save_autostart", { on: enabled });
    }
    if (!isMac && enabled) {
      // Refresh the Run-key entry so a moved exe still resolves (P1-30).
      try { await enable(); } catch (e) { console.warn("autostart refresh failed", e); }
    }
  } catch {
    autostartBox.checked = !!snap.config.autostart;
  }
  $("xmbc-banner").classList.toggle("hidden", !snap.xmbc_running);

  const modeTag = snap.is_portable ? "[便携模式] " : "[标准安装] ";
  const pathEl = $("cfg-path");
  $("cfg-path-text").textContent = `${modeTag}${snap.config_dir}`;
  pathEl.title = isMac
    ? "点击在访达中定位配置目录"
    : "点击在文件资源管理器中定位配置目录";
  pathEl.addEventListener("click", async () => {
    await safeInvoke("open_config_dir");
  });

  const hotkeys = snap.emergency_hotkeys ?? 0;
  if (isMac) {
    $("safety-copy").textContent = "左右键保留原操作 · 急停 F13 或 ⌃⌥⌘P";
    document.querySelector<HTMLButtonElement>('[data-key="LWin"]')!.textContent = "⌘ Command";
  } else if ((hotkeys & 1) === 0 && (hotkeys & 2) === 0) {
    $("safety-copy").textContent = "左右键保留原操作 · ⚠ 急停热键被占用，请从托盘暂停";
  } else if ((hotkeys & 1) === 0 || (hotkeys & 2) === 0) {
    const usable = (hotkeys & 1) !== 0 ? "Pause" : "Scroll Lock";
    $("safety-copy").textContent = `左右键保留原操作 · 急停 ${usable}`;
  }
  paintDots("");
  renderMaps();

  if (snap.last) {
    $("pulse-name").textContent = BUTTON_LABEL[snap.last.button] ?? snap.last.button;
    highlightMouse(snap.last.button);
    paintDots(snap.last.button);
  }

  for (const b of snap.active_bindings ?? []) {
    runtimeStates.set(b.button, { active: b.active, mode: b.mode });
  }
  updateAllRuntimePills();

  if (snap.listening && !listenGen) {
    listenGen = ++localGen;
    $("listen-copy").textContent = "请按侧键、中键或滚轮。15 秒后自动结束识别。";
    $("btn-listen").textContent = "取消识别";
    document.querySelector(".listen-sheet")?.classList.add("armed");
  }

  if (snap.recovery_notes?.length) {
    showAlert(snap.recovery_notes.join("；"), {
      sticky: true,
      kind: "recovery",
      action: {
        label: "知道了",
        onClick: () => {
          void safeInvoke("clear_recovery_notes");
          hideAlert();
        },
      },
    });
  }

  // Scrollspy: light 偏好设置 while the settings dock is in view, 鼠标映射 otherwise.
  // Threshold (not a rootMargin band) is used because the dock is the last child of
  // .stage — it can never reach a mid-viewport band at max scroll.
  if (!scrollSpyStarted) {
    scrollSpyStarted = true;
    const stageEl = document.querySelector<HTMLElement>(".stage");
    const dockEl = document.getElementById("settings");
    if (stageEl && dockEl) {
      const setActiveRailLink = (hash: string) => {
        document.querySelectorAll<HTMLAnchorElement>(".rail-link").forEach((el) => {
          el.classList.toggle("active", el.getAttribute("href") === hash);
        });
        syncRailThumb();
      };
      const spy = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (entry.target !== dockEl) continue;
            // scrollTop guard: when everything fits without scrolling, keep 鼠标映射.
            const inSettings =
              entry.intersectionRatio >= 0.15 && stageEl.scrollTop > 8;
            setActiveRailLink(inSettings ? "#settings" : "#workspace");
          }
        },
        { root: stageEl, threshold: [0, 0.15] }
      );
      spy.observe(dockEl);
    }
  }
}

$("btn-theme").addEventListener("click", async () => {
  const button = $("btn-theme") as HTMLButtonElement;
  const previous = document.documentElement.dataset.theme || "light";
  const next = previous === "dark" ? "light" : "dark";
  button.disabled = true;
  const doApply = () => { applyTheme(next); };
  if (reducedMotion.matches || !("startViewTransition" in document)) {
    doApply();
  } else {
    // Shrink the old theme into a circle centered on the button, revealing the
    // new one — reads as the new look radiating out of the toggle.
    const r = button.getBoundingClientRect();
    const x = r.left + r.width / 2;
    const y = r.top + r.height / 2;
    const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
    try {
      const vt = (document as any).startViewTransition(doApply);
      vt.ready.then(() => {
        // The new theme grows outward as a circle centered on the button.
        document.documentElement.animate(
          { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
          { duration: 450, easing: "ease-out", pseudoElement: "::view-transition-new(root)" }
        );
      }).catch(() => {});
    } catch {
      doApply();
    }
  }
  try { await invokeT("save_theme", { theme: next }); }
  catch (err) { applyTheme(previous); showAlert(`主题保存失败：${String(err)}`, { kind: "save" }); }
  finally { button.disabled = false; }
});

$("btn-pause").addEventListener("click", async () => {
  const next = !isPaused;
  const button = $("btn-pause") as HTMLButtonElement;
  button.disabled = true;
  try {
    await invokeT("save_paused", { paused: next });
    setPausedUi(next);
    hideAlert("save");
  } catch (err) { showAlert(`暂停状态保存失败：${String(err)}`, { kind: "save" }); }
  finally { button.disabled = false; }
});

$("btn-listen").addEventListener("click", async () => {
  if (listenGen) { await stopListening(); return; }
  if (!hookReady) {
    showAlert("监听尚未就绪，暂时无法识别。");
    return;
  }
  const gen = ++localGen;
  listenGen = gen;
  $("listen-copy").textContent = "请按侧键、中键或滚轮。15 秒后自动结束识别。";
  $("btn-listen").textContent = "取消识别";
  document.querySelector(".listen-sheet")?.classList.add("armed");
  try {
    await invokeT("arm_listen");
    if (listenGen !== gen) return;
    if (!document.hasFocus()) {
      await stopListening();
      return;
    }
    listenTimer = window.setTimeout(() => {
      if (listenGen === gen) {
        void stopListening();
        showAlert("15 秒内未检测到鼠标按键，识别已结束。");
      }
    }, 15000);
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
  for (const key of keys[preset]) await safeInvoke("add_record_key", { key });
});

let quitArmed = false;
$("btn-quit").addEventListener("click", async () => {
  const btn = $("btn-quit") as HTMLButtonElement;
  if (!quitArmed) {
    quitArmed = true;
    btn.textContent = "再点一次确认退出";
    window.setTimeout(() => {
      quitArmed = false;
      btn.textContent = "退出应用";
    }, 3000);
    return;
  }
  await safeInvoke("quit_app");
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
    const changed = applyButtonChange(m, sel.value, mappings);
    if (!changed) {
      sel.value = m.button;
      return;
    }
    runtimeStates.delete(m.button);
    Object.assign(m, changed);
  }
  if (sel.dataset.k === "mode") {
    Object.assign(m, applyModeChange(m, sel.value as "toggle" | "click" | "dual"));
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
    if (!target) return;
    const removed = structuredClone(target);
    runtimeStates.delete(target.button);
    mappings = mappings.filter((m) => m.id !== id);
    if (selectedMappingId === id) {
      selectedMappingId = mappings[0]?.id ?? null;
    }
    await persist();
    showAlert(`已删除「${BUTTON_LABEL[target.button] ?? target.button}」的映射`, {
      timeout: 5000,
      kind: "undo",
      action: {
        label: "撤销",
        onClick: () => {
          if (!mappings.some((m) => m.id === removed.id)) {
            mappings.push(removed);
            selectedMappingId = removed.id;
            void persist();
          }
          hideAlert();
        },
      },
    });
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
      Object.assign(target, removeKeyFromSlot(target, slot, removeKey));
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
  updateRecordOk();
  await safeInvoke("remove_record_key", { key });
});

$("record-chips").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const key = t.dataset.key;
  if (!key || !currentDraft) return;
  await safeInvoke("add_record_key", { key });
});

window.addEventListener(
  "keydown",
  (e) => {
    if (!currentDraft) return;
    if (
      (e.key === "Enter" || e.key === " ") &&
      (e.target as HTMLElement).closest?.("#record-mask button")
    ) {
      return;
    }
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
    if (hookReady) return; // The low-level hook records real key-down events.
    const token = codeToToken(e.code);
    if (!token) return;
    void safeInvoke("press_record_key", { key: token, down: true });
  },
  true
);

window.addEventListener(
  "keyup",
  (e) => {
    if (!currentDraft || hookReady) return;
    const token = codeToToken(e.code);
    if (!token) return;
    void safeInvoke("press_record_key", { key: token, down: false });
  },
  true
);

let autostartBusy = false;
$("chk-autostart").addEventListener("change", async (e) => {
  const box = e.target as HTMLInputElement;
  if (autostartBusy) {
    box.checked = !box.checked;
    return;
  }
  autostartBusy = true;
  const on = box.checked;
  try {
    const { enable, disable } = await import("@tauri-apps/plugin-autostart");
    if (on) await enable();
    else await disable();
    await invokeT("save_autostart", { on });
  } catch (err) {
    try {
      const { isEnabled } = await import("@tauri-apps/plugin-autostart");
      box.checked = await isEnabled();
    } catch {
      box.checked = !on;
    }
    showAlert(`开机自启设置失败：${String(err)}`);
  } finally {
    autostartBusy = false;
  }
});

async function checkForUpdate(current: string) {
  const status = $("update-status");
  const button = $("btn-check-update") as HTMLButtonElement;
  if (button.disabled) return;
  button.disabled = true;
  status.textContent = "正在检查更新…";
  try {
    const release = await latestRelease();
    const newer = isNewer(release.version, current);
    status.textContent = newer ? `可更新至 ${release.version}` : "当前已是最新版本";
    if (newer) {
      const link = document.createElement("button");
      link.className = "ghost";
      link.textContent = "查看发行说明";
      link.addEventListener("click", () => { void safeInvoke("open_url", { url: release.url }); });
      status.appendChild(link);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : "";
    status.textContent =
      msg === "NO_RELEASE"
        ? "尚未发布正式更新"
        : msg === "RATE_LIMITED"
          ? "GitHub 接口暂时限流，请稍后再试"
          : msg === "TIMEOUT"
            ? "连接超时"
            : "暂时无法连接 GitHub";
    const link = document.createElement("button");
    link.className = "ghost";
    link.textContent = "打开下载页面";
    link.addEventListener("click", () => { void safeInvoke("open_url", { url: RELEASES_URL }); });
    status.appendChild(link);
  } finally {
    button.disabled = false;
  }
}

$("xmbc-recheck").addEventListener("click", async () => {
  try {
    const running = await invokeT<boolean>("xmbc_running");
    $("xmbc-banner").classList.toggle("hidden", !running);
  } catch (err) {
    console.error("xmbc_running check failed:", err);
  }
});

window.addEventListener("blur", () => {
  if (currentDraft) {
    const count = recordBuf.length;
    void closeRecord();
    if (count) {
      showAlert(`窗口失去焦点，录制已取消（已录 ${count} 键未保存）。`);
    }
  }
  if (listenGen) void stopListening();
});
window.addEventListener("unhandledrejection", (event) => {
  console.error("unhandled rejection:", event.reason);
  if (event.reason instanceof Error && event.reason.name === "AbortError") return;
  showAlert(`操作未完成：${String(event.reason)}`);
});

let currentVersion = "0.0.0";
$("btn-check-update").addEventListener("click", () => {
  void checkForUpdate(currentVersion);
});
void (async () => {
  try {
    currentVersion = await invokeT<string>("app_version");
    $("app-version").textContent = `v${currentVersion}`;
  } catch {
    $("app-version").textContent = "版本未知";
  }
})();

function bootFailed(err: unknown) {
  const host = $("maps");
  host.replaceChildren();
  const p = document.createElement("p");
  p.className = "empty-state";
  p.textContent = `无法连接映射引擎：${String(err)}`;
  const retry = document.createElement("button");
  retry.className = "solid";
  retry.id = "btn-retry-boot";
  retry.textContent = "重试";
  retry.addEventListener("click", () => {
    void boot().catch(bootFailed);
  });
  host.append(p, retry);
  showAlert(`无法连接映射引擎，请重新打开控制面板：${String(err)}`);
}

void boot().catch(bootFailed);

type CardDrag = { x: number; y: number; lastX: number; lastY: number; lastTime: number; velocity: number; dx: number; dy: number; id: number; captureEl: HTMLElement; card: HTMLElement };
let drag: CardDrag | null = null;
let deckBusy = false;
let deckEpoch = 0;
let queuedDir = 0;
let dragFrame = 0;
const deckAnimations = new Set<Animation>();
let preview: HTMLElement | null = null;
let previewTarget: HTMLElement | null = null;

function resetDeck() {
  ++deckEpoch;
  const previous = drag;
  drag = null;
  if (previous?.captureEl.hasPointerCapture(previous.id)) previous.captureEl.releasePointerCapture(previous.id);
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
  const mapsHost = $("maps");
  mapsHost.style.removeProperty("min-height");
  mapsHost.style.removeProperty("--deck-dx");
  mapsHost.style.removeProperty("--deck-dy");
  mapsHost.style.removeProperty("--deck-p");
  mapsHost.classList.remove("deck-moving");
  deckBusy = false;
  queuedDir = 0;
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
  preview.className = `deck-preview selected${target.classList.contains("collapsed") ? " collapsed" : ""}`;
  preview.removeAttribute("data-id");
  preview.removeAttribute("data-button");
  preview.inert = true;
  preview.setAttribute("aria-hidden", "true");
  $("maps").append(preview);
}
function paintDrag() {
  dragFrame = 0;
  if (!drag) return;
  const vertical = Math.abs(drag.dy) > Math.abs(drag.dx);
  preparePreview((vertical ? drag.dy : drag.dx) >= 0 ? 1 : -1);
  const mapsHost = $("maps");
  const width = mapsHost.clientWidth;
  const dx = Math.max(-width * .45, Math.min(width * .45, drag.dx * .65));
  const dy = Math.max(-130, Math.min(130, drag.dy * .65));
  const progress = Math.min(1, Math.hypot(drag.dx, drag.dy) / 240);
  mapsHost.style.setProperty("--deck-dx", `${dx.toFixed(1)}px`);
  mapsHost.style.setProperty("--deck-dy", `${dy.toFixed(1)}px`);
  mapsHost.style.setProperty("--deck-p", progress.toFixed(3));
  drag.card.style.transform = `perspective(1000px) translate3d(${dx}px, ${dy}px, ${progress * 36}px) rotateX(${-dy / 26}deg) rotateY(${dx / 65}deg) rotate(${dx / width * 4}deg) scale(${1 + progress * .025})`;
  drag.card.style.filter = reducedMotion.matches ? "none" : `blur(${progress * 3}px)`;
  if (preview) {
    preview.style.transform = `translate3d(${-dx * .08}px, ${18 * (1 - progress) - dy * .08}px, 0) scale(${.94 + .06 * progress})`;
    preview.style.filter = reducedMotion.matches ? "none" : `blur(${5 * (1 - progress)}px)`;
    preview.style.opacity = String(.5 + .5 * progress);
  }
}
async function animateDeck(el: HTMLElement, frames: Keyframe[], duration: number) {
  if (reducedMotion.matches) return;
  const animation = el.animate(frames, { duration, easing: "cubic-bezier(.22,.75,.25,1)", fill: "forwards" });
  deckAnimations.add(animation);
  try { await animation.finished; } catch { /* Cancelled by blur, capture loss or a rerender. */ }
}
async function cycleCard(direction: number, fromDrag = false, vertical = false) {
  if (currentDraft) return; // Recording keeps the old early-return; nothing is queued.
  if (deckBusy) {
    queuedDir = Math.sign(queuedDir + direction);
    return;
  }
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
      { transform: `translate3d(${vertical ? 0 : direction * Math.min(260, host.clientWidth * .32)}px, ${vertical ? direction * 150 : -12}px, 0) rotate(${direction * 5}deg) scale(1.045)`, filter: "blur(6px)", opacity: .25, offset: .55 },
      { transform: `translate3d(${vertical ? 0 : direction * 60}px, ${vertical ? direction * 45 : 20}px, 0) scale(.92)`, filter: "blur(9px)", opacity: 0 }
    ], 420),
    ...(preview ? [animateDeck(preview, [
      { transform: preview.style.transform || "translateY(18px) scale(.94)", filter: preview.style.filter || "blur(5px)", opacity: preview.style.opacity || .5 },
      { transform: `translateY(-3px) scale(1.012)`, filter: "blur(0px)", opacity: 1, offset: .72 },
      { transform: "none", filter: "blur(0px)", opacity: 1 }
    ], 420)] : [])
  ]);
  if (epoch !== deckEpoch) return;
  // Capture the queued direction before resetDeck wipes it, fire after the reorder.
  const followUp = queuedDir;
  queuedDir = 0;
  resetDeck();
  if (direction > 0) host.append(first); else host.prepend(next);
  selectMapping(next.dataset.id!);
  if (followUp) void cycleCard(followUp);
}
function bindDeckButton(btn: HTMLElement, direction: number) {
  let delayTimer = 0;
  let repeatTimer = 0;
  const stopRepeat = () => {
    window.clearTimeout(delayTimer);
    window.clearInterval(repeatTimer);
    delayTimer = repeatTimer = 0;
  };
  btn.addEventListener("pointerdown", e => {
    if (e.button !== 0 || !e.isPrimary) return;
    stopRepeat();
    void cycleCard(direction); // deckBusy inside cycleCard self-locks repeats
    delayTimer = window.setTimeout(() => {
      repeatTimer = window.setInterval(() => { void cycleCard(direction); }, 260);
    }, 450);
  });
  btn.addEventListener("pointerup", stopRepeat);
  btn.addEventListener("pointerleave", stopRepeat);
  btn.addEventListener("pointercancel", stopRepeat);
  // Keyboard activation produces a click with detail === 0; pointerdown already handled mouse input.
  btn.addEventListener("click", e => { if (e.detail === 0) void cycleCard(direction); });
}
bindDeckButton($("deck-prev"), -1);
bindDeckButton($("deck-next"), 1);
$("maps").addEventListener("pointerdown", e => {
  if (e.button !== 0 || !e.isPrimary || deckBusy || mappings.length < 2) return;
  const t = e.target as HTMLElement;
  if (t.closest("button, select, input, a, .key, .chip, [contenteditable]")) return;
  const card = t.closest<HTMLElement>(".map");
  if (!card || card.parentElement !== $("maps")) return;
  resetDeck();
  drag = { x: e.clientX, y: e.clientY, lastX: e.clientX, lastY: e.clientY, lastTime: e.timeStamp, velocity: 0, dx: 0, dy: 0, id: e.pointerId, captureEl: card, card };
  card.setPointerCapture(e.pointerId);
  $("maps").classList.add("deck-moving");
});
$("maps").addEventListener("pointermove", e => {
  if (!drag || e.pointerId !== drag.id) return;
  if (!(e.buttons & 1)) { resetDeck(); return; }
  const elapsed = e.timeStamp - drag.lastTime;
  if (elapsed > 0) drag.velocity = Math.hypot(e.clientX - drag.lastX, e.clientY - drag.lastY) / elapsed;
  drag.lastX = e.clientX;
  drag.lastY = e.clientY;
  drag.lastTime = e.timeStamp;
  drag.dx = e.clientX - drag.x;
  drag.dy = e.clientY - drag.y;
  if (!dragFrame) dragFrame = requestAnimationFrame(paintDrag);
});
$("maps").addEventListener("pointerup", e => {
  if (!drag || e.pointerId !== drag.id) return;
  cancelAnimationFrame(dragFrame);
  drag.dx = e.clientX - drag.x;
  drag.dy = e.clientY - drag.y;
  paintDrag();
  const finished = drag;
  drag = null; // Clear before releasing capture: lostpointercapture must not cancel the settle.
  if (finished.captureEl.hasPointerCapture(e.pointerId)) finished.captureEl.releasePointerCapture(e.pointerId);
  const vertical = Math.abs(finished.dy) > Math.abs(finished.dx);
  const distance = Math.hypot(finished.dx, finished.dy);
  const flick = e.timeStamp - finished.lastTime < 100 && Math.abs(finished.velocity) > .5 && distance > 25;
  if (distance >= Math.min(110, $("maps").clientWidth * .18) || flick) {
    void cycleCard((vertical ? finished.dy : finished.dx) >= 0 ? 1 : -1, true, vertical);
  } else {
    const epoch = deckEpoch;
    deckBusy = true;
    void Promise.all([animateDeck(finished.card, [
      { transform: finished.card.style.transform, filter: finished.card.style.filter },
      { transform: "none", filter: "blur(0px)" }
    ], 140), ...(preview ? [animateDeck(preview, [
      { transform: preview.style.transform, filter: preview.style.filter, opacity: preview.style.opacity },
      { transform: "translateY(18px) scale(.94)", filter: "blur(5px)", opacity: .5 }
    ], 140)] : [])]).then(() => {
      if (epoch !== deckEpoch) return;
      const followUp = queuedDir;
      queuedDir = 0;
      resetDeck();
      if (followUp) void cycleCard(followUp);
    });
  }
});
$("maps").addEventListener("pointercancel", resetDeck);
$("maps").addEventListener("lostpointercapture", () => { if (drag) resetDeck(); });
window.addEventListener("blur", resetDeck);
window.addEventListener("keydown", () => { if (drag || deckBusy) resetDeck(); });
window.addEventListener("resize", resetDeck);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    pendingPulses.clear();
    pendingDown = null;
    wheelHoldButton = null;
    clearTimeout(wheelHoldTimer);
    resetDeck();
  }
});
reducedMotion.addEventListener("change", resetDeck);
// --- Draggable liquid-glass rail thumb -------------------------------------
// .rail-thumb is created in JS and positioned absolutely by CSS; it never
// participates in the rail's flex layout and doesn't depend on .app-brand.
const railEl = document.querySelector<HTMLElement>(".rail");
let railThumb: HTMLElement | null = null;
let railDrag: { id: number; startX: number; startLeft: number; min: number; max: number } | null = null;

function railLinks(): HTMLElement[] {
  return railEl ? [...railEl.querySelectorAll<HTMLElement>(".rail-link")] : [];
}

/** Link's left edge relative to .rail, walking offsetParents (nav may sit between). */
function railLinkLeft(link: HTMLElement): number {
  let left = link.offsetLeft;
  let parent = link.offsetParent as HTMLElement | null;
  while (parent && railEl && parent !== railEl) {
    left += parent.offsetLeft;
    parent = parent.offsetParent as HTMLElement | null;
  }
  return left;
}

function positionRailThumb() {
  if (!railEl) return;
  if (!railThumb) {
    railThumb = document.createElement("div");
    railThumb.className = "rail-thumb";
    railThumb.setAttribute("aria-hidden", "true");
    railThumb.style.opacity = "0"; // hidden until the first successful measure
    railEl.prepend(railThumb);
    bindRailThumbDrag(railThumb);
  }
  if (railDrag) return; // don't fight an in-progress drag
  const link =
    railEl.querySelector<HTMLElement>(".rail-link.active") ?? railLinks()[0];
  if (!link) return;
  railThumb.style.width = `${link.offsetWidth}px`;
  railThumb.style.transform = `translateX(${railLinkLeft(link)}px)`;
  railThumb.style.opacity = "1";
}

/** Single call site to keep the thumb under .rail-link.active. */
function syncRailThumb() {
  positionRailThumb();
}

// The thumb sits behind the links (z-index), so pointer events always land on
// a .rail-link. Dragging therefore starts on the links themselves: beyond a 6px
// threshold it becomes a thumb drag, otherwise the press stays a plain click.
let railSuppressClick = false;

function bindRailThumbDrag(thumb: HTMLElement) {
  if (!railEl) return;
  let pending: { id: number; startX: number; link: HTMLElement } | null = null;

  const startDrag = (e: PointerEvent) => {
    if (!pending || !railEl) return;
    const links = railLinks();
    if (!links.length) return;
    const minLeft = Math.min(...links.map(railLinkLeft));
    const maxRight = Math.max(...links.map((l) => railLinkLeft(l) + l.offsetWidth));
    const width = thumb.offsetWidth || links[0].offsetWidth;
    const railRect = railEl.getBoundingClientRect();
    railDrag = {
      id: pending.id,
      startX: pending.startX,
      startLeft: thumb.getBoundingClientRect().left - railRect.left,
      min: minLeft,
      max: Math.max(minLeft, maxRight - width),
    };
    pending = null;
    thumb.classList.add("dragging");
    railSuppressClick = true;
  };

  railEl.addEventListener("pointerdown", (e) => {
    const link = (e.target as HTMLElement).closest<HTMLElement>(".rail-link");
    if (!link || e.button !== 0 || !e.isPrimary) return;
    pending = { id: e.pointerId, startX: e.clientX, link };
    link.setPointerCapture(e.pointerId);
  });
  railEl.addEventListener("pointermove", (e) => {
    if (pending && e.pointerId === pending.id && Math.abs(e.clientX - pending.startX) > 6) {
      startDrag(e);
    }
    if (!railDrag || e.pointerId !== railDrag.id) return;
    const left = Math.min(
      railDrag.max,
      Math.max(railDrag.min, railDrag.startLeft + e.clientX - railDrag.startX)
    );
    thumb.style.transform = `translateX(${left}px)`;
  });
  const endDrag = (e: PointerEvent, cancelled: boolean) => {
    if (pending && e.pointerId === pending.id) pending = null;
    if (!railDrag || e.pointerId !== railDrag.id) return;
    railDrag = null;
    thumb.classList.remove("dragging");
    if (cancelled || !railEl) {
      positionRailThumb(); // snap back under the active link
      return;
    }
    const links = railLinks();
    if (!links.length) return;
    const railRect = railEl.getBoundingClientRect();
    const rect = thumb.getBoundingClientRect();
    const center = rect.left - railRect.left + rect.width / 2;
    const target =
      links.find((l) => center >= railLinkLeft(l) && center <= railLinkLeft(l) + l.offsetWidth) ??
      links.reduce((best, l) =>
        Math.abs(railLinkLeft(l) + l.offsetWidth / 2 - center) <
        Math.abs(railLinkLeft(best) + best.offsetWidth / 2 - center)
          ? l
          : best
      );
    // Reuse the link's click handler: smooth scroll + active + thumb sync.
    target.click();
  };
  railEl.addEventListener("pointerup", (e) => endDrag(e, false));
  railEl.addEventListener("pointercancel", (e) => endDrag(e, true));
}

if (railEl) {
  positionRailThumb();
  // Re-measure once fonts/layout settle so the first position is exact.
  requestAnimationFrame(() => requestAnimationFrame(positionRailThumb));
  document.fonts?.ready.then(positionRailThumb).catch(() => {});
  window.addEventListener("resize", positionRailThumb);
}

document.querySelectorAll<HTMLAnchorElement>(".rail-link").forEach(link => {
  link.addEventListener("click", e => {
    // A finished thumb drag produces a synthetic-looking release click on the
    // pressed link — swallow it; only programmatic target.click() (untrusted)
    // is allowed through.
    if (railSuppressClick && e.isTrusted) {
      railSuppressClick = false;
      e.preventDefault();
      return;
    }
    const href = link.getAttribute("href");
    if (href === "#workspace") {
      // #workspace is the .stage scroll container itself — the default anchor jump is a no-op.
      e.preventDefault();
      document.querySelector<HTMLElement>(".stage")?.scrollTo({
        top: 0,
        behavior: reducedMotion.matches ? "auto" : "smooth",
      });
    } else if (href === "#settings") {
      // scroll-margin on the dock supplies the breathing room.
      e.preventDefault();
      document.getElementById("settings")?.scrollIntoView({
        behavior: reducedMotion.matches ? "auto" : "smooth",
        block: "start",
      });
    }
    document.querySelectorAll(".rail-link").forEach(el => el.classList.toggle("active", el === link));
    syncRailThumb();
  });
});
