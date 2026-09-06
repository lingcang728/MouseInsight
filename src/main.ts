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

function renderKeys(el: HTMLElement, keys: string[], dimEmpty = true) {
  const labels = prettyKeys(keys);
  el.replaceChildren();
  labels.forEach((k) => {
    const span = document.createElement("span");
    span.className = `key${keys.length || !dimEmpty ? "" : " dim"}`;
    span.textContent = k;
    el.appendChild(span);
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
  } else if (m.mode === "hold") {
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
  $("dock-lead").textContent = `${BUTTON_LABEL[selected.button] ?? selected.button} · ${MODE_LABEL[selected.mode] ?? selected.mode}`;
  renderKeys($("dock-keys"), selected.keys);
  const hintEl = $("dock-hint");
  if (hintEl) {
    hintEl.textContent = `${MODE_LABEL[selected.mode] ?? selected.mode}：${MODE_DESC[selected.mode] ?? ""}`;
  }
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

    // 鼠标按钮下拉框：严格限定为 MAPPABLE_BUTTONS，排除 left 和 right
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

    // 模式选择器：滚轮（wheelup/wheeldown）仅允许 click（单次触发）
    const selMode = document.createElement("select");
    selMode.dataset.k = "mode";
    selMode.className = "map-sel-mode";
    const availableModes = getAllowedModesForButton(m.button);
    availableModes.forEach((modeKey) => {
      const opt = document.createElement("option");
      opt.value = modeKey;
      opt.textContent = MODE_LABEL[modeKey] ?? modeKey;
      opt.selected = m.mode === modeKey;
      selMode.appendChild(opt);
    });

    // 状态指示 pill
    const pill = document.createElement("span");
    const pillData = getStatusPill(m);
    pill.className = pillData ? pillData.className : "pill status-pill hidden";
    pill.textContent = pillData ? pillData.text : "";

    // 录键按钮
    const btnRecord = document.createElement("button");
    btnRecord.type = "button";
    btnRecord.className = "ghost map-btn-record";
    btnRecord.dataset.act = "record";
    const keyLabels = prettyKeys(m.keys).join(" + ");
    btnRecord.textContent = m.keys.length ? keyLabels : "录快捷键";

    // 删除按钮
    const btnDel = document.createElement("button");
    btnDel.type = "button";
    btnDel.className = "danger";
    btnDel.dataset.act = "del";
    btnDel.setAttribute("aria-label", "删除");
    btnDel.textContent = "删除";

    row.append(selButton, selMode, pill, btnRecord, btnDel);

    // 模式说明文案
    const desc = document.createElement("div");
    desc.className = "map-desc mode-hint";
    desc.textContent = `${MODE_LABEL[m.mode] ?? m.mode}：${MODE_DESC[m.mode] ?? ""}`;

    article.append(row, desc);
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

async function openRecordForExisting(id: string) {
  const m = mappings.find((x) => x.id === id);
  if (!m) return;
  currentDraft = {
    button: m.button,
    isNew: false,
    existingId: m.id,
    initialKeys: [...m.keys],
  };
  recordBuf = [...m.keys];
  $("record-mask").classList.remove("hidden");
  renderKeys($("record-keys"), recordBuf, false);
  await invoke("arm_record");
}

async function openRecordForNew(button: string) {
  currentDraft = {
    button,
    isNew: true,
    initialKeys: [],
  };
  recordBuf = [];
  $("record-mask").classList.remove("hidden");
  renderKeys($("record-keys"), []);
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

  if (currentDraft.isNew) {
    const inferredMode = inferTriggerMode(currentDraft.button, finalKeys);
    const newMapping: Mapping = {
      id: crypto.randomUUID(),
      button: currentDraft.button,
      mode: inferredMode,
      keys: [...finalKeys],
    };
    mappings.push(newMapping);
    selectedMappingId = newMapping.id;
  } else {
    const existing = mappings.find((x) => x.id === currentDraft?.existingId);
    if (existing) {
      existing.keys = [...finalKeys];
      if (
        (existing.button === "wheelup" || existing.button === "wheeldown") &&
        existing.mode !== "click"
      ) {
        existing.mode = "click";
      }
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
    if (existing) {
      selectedMappingId = existing.id;
      renderMaps(capturedButton);
      await openRecordForExisting(existing.id);
    } else {
      // 事务式草稿：不添加进 mappings，不保存
      await openRecordForNew(capturedButton);
    }
  });

  await listen<string[]>("record-keys", (ev) => {
    recordBuf = ev.payload ?? [];
    renderKeys($("record-keys"), recordBuf);
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
    }
  }
  if (sel.dataset.k === "mode") {
    m.mode = sel.value;
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

  if (t.dataset.act === "record") {
    selectedMappingId = id;
    renderMaps();
    openRecordForExisting(id);
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

boot().catch((err) => {
  console.error("MouseInsight 初始化异常:", err);
});
