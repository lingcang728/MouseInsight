import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Mapping = {
  id: string;
  button: string;
  mode: string;
  keys: string[];
  label?: string;
};

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

const BUTTON_LABEL: Record<string, string> = {
  left: "左键",
  right: "右键",
  middle: "中键",
  xbutton1: "侧键 · 后",
  xbutton2: "侧键 · 前",
  wheelup: "滚轮上",
  wheeldown: "滚轮下",
};

const MODE_LABEL: Record<string, string> = {
  hold: "按住",
  click: "点按",
  toggle: "开关",
};

let mappings: Mapping[] = [];
let selectedMappingId: string | null = null;
let recordingFor: string | null = null;
let recordBuf: string[] = [];
let isPaused = false;

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
}

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
  }
  const dots = [...host.children] as HTMLElement[];
  const col = { xbutton1: 1, xbutton2: 3, middle: 7, wheelup: 9, wheeldown: 10, left: 12, right: 14 }[
    button
  ];
  dots.forEach((d, i) => {
    d.className = "dot";
    const c = i % 16;
    if (col !== undefined && Math.abs(c - col) < 2) d.classList.add("lit");
    if (c === col) d.classList.add("hot");
  });
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
  $("dock-lead").textContent = `${BUTTON_LABEL[selected.button] ?? selected.button} · ${MODE_LABEL[selected.mode] ?? selected.mode}`;
  renderKeys($("dock-keys"), selected.keys);
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

    // Button selector with duplicate prevention
    const selButton = document.createElement("select");
    selButton.dataset.k = "button";
    Object.entries(BUTTON_LABEL).forEach(([v, l]) => {
      const opt = document.createElement("option");
      opt.value = v;
      const isOccupied = mappings.some((other) => other.id !== m.id && other.button === v);
      opt.textContent = isOccupied ? `${l} (已绑定)` : l;
      opt.selected = m.button === v;
      opt.disabled = isOccupied;
      selButton.appendChild(opt);
    });

    // Mode selector
    const selMode = document.createElement("select");
    selMode.dataset.k = "mode";
    Object.entries(MODE_LABEL).forEach(([v, l]) => {
      const opt = document.createElement("option");
      opt.value = v;
      opt.textContent = l;
      opt.selected = m.mode === v;
      selMode.appendChild(opt);
    });

    // Key record button
    const btnRecord = document.createElement("button");
    btnRecord.type = "button";
    btnRecord.className = "ghost";
    btnRecord.dataset.act = "record";
    const keyLabels = prettyKeys(m.keys).join(" + ");
    btnRecord.textContent = m.keys.length ? keyLabels : "录快捷键";

    // Delete button
    const btnDel = document.createElement("button");
    btnDel.type = "button";
    btnDel.className = "danger";
    btnDel.dataset.act = "del";
    btnDel.setAttribute("aria-label", "删除");
    btnDel.textContent = "删除";

    article.append(selButton, selMode, btnRecord, btnDel);
    host.appendChild(article);
  });

  renderDock();
}

async function persist() {
  try {
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

async function openRecord(id: string) {
  recordingFor = id;
  recordBuf = [];
  $("record-mask").classList.remove("hidden");
  renderKeys($("record-keys"), []);
  await invoke("arm_record");
}

async function closeRecord() {
  recordingFor = null;
  $("record-mask").classList.add("hidden");
  await invoke("disarm_record");
}

async function boot() {
  const snap = await invoke<Snapshot>("get_snapshot");
  const seenButtons = new Set<string>();
  mappings = (snap.config.mappings ?? []).filter((m) => {
    if (seenButtons.has(m.button)) return false;
    seenButtons.add(m.button);
    return true;
  });
  if (mappings.length) {
    selectedMappingId = mappings[0].id;
  }

  applyTheme(snap.config.theme || "dark");
  setPausedUi(!!snap.config.paused);
  ($("chk-autostart") as HTMLInputElement).checked = !!snap.config.autostart;
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

  await listen<Pulse>("mouse-pulse", (ev) => {
    const p = ev.payload;
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
  });

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
    $("listen-copy").textContent = `听到了：${BUTTON_LABEL[ev.payload] ?? ev.payload}。请录键盘。`;
    $("btn-listen").textContent = "再听一颗";
    document.querySelector(".listen-sheet")?.classList.remove("armed");
    let m = mappings.find((x) => x.button === ev.payload);
    if (!m) {
      m = {
        id: crypto.randomUUID(),
        button: ev.payload,
        mode: "hold",
        keys: [],
      };
      mappings.push(m);
      await persist();
    }
    selectedMappingId = m.id;
    renderMaps(ev.payload);
    await openRecord(m.id);
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
  $("listen-copy").textContent = "在听。按鼠标上你要绑定的那颗键，左右键也可以。";
  $("btn-listen").textContent = "在听…";
  document.querySelector(".listen-sheet")?.classList.add("armed");
  await invoke("arm_listen");
});

// Quit button: bound only ONCE
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
  }
  if (sel.dataset.k === "mode") {
    m.mode = sel.value;
  }
  await persist();
  renderMaps();
});

$("maps").addEventListener("click", async (e) => {
  const t = e.target as HTMLElement;
  const row = t.closest(".map") as HTMLElement | null;
  if (!row) return;
  const id = row.dataset.id!;

  if (t.dataset.act === "del") {
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
    openRecord(id);
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
  const keys = await invoke<string[]>("take_record_keys");
  const use = keys.length ? keys : recordBuf;
  if (recordingFor && use.length) {
    const m = mappings.find((x) => x.id === recordingFor);
    if (m) m.keys = [...use];
    await persist();
    renderMaps();
  }
  await closeRecord();
});

$("record-mask").addEventListener("click", (e) => {
  if (e.target === $("record-mask")) {
    closeRecord();
  }
});

window.addEventListener("blur", () => {
  if (recordingFor) {
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
    if (!recordingFor) return;
    e.preventDefault();
    e.stopPropagation();
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
