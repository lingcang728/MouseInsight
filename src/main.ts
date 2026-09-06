import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Mapping = {
  id: string;
  button: string;
  mode: string;
  keys: string[];
  label: string;
};

type Pulse = {
  button: string;
  down: boolean;
  t: number;
  swallowed: boolean;
};

type Snapshot = {
  config: {
    theme: string;
    autostart: boolean;
    paused: boolean;
    mappings: Mapping[];
  };
  xmbc_running: boolean;
  last: Pulse | null;
  listening: boolean;
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
let recordingFor: string | null = null;
let recordBuf: string[] = [];
let listening = false;

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

function uid() {
  return Math.random().toString(36).slice(2, 10);
}

function prettyKeys(keys: string[]) {
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
  el.innerHTML = labels
    .map((k) => `<span class="key${keys.length || !dimEmpty ? "" : " dim"}">${k}</span>`)
    .join("");
}

function applyTheme(theme: string) {
  document.documentElement.dataset.theme = theme;
  $("btn-theme").textContent = theme === "dark" ? "换成浅色" : "换成深色";
}

function setPausedUi(paused: boolean) {
  $("btn-pause").textContent = paused ? "继续映射" : "暂停映射";
}

function paintDots(button: string) {
  const host = $("dots");
  if (!host.childElementCount) {
    host.innerHTML = Array.from({ length: 16 * 4 }, () => `<i class="dot"></i>`).join("");
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

function renderMaps(liveButton?: string) {
  const host = $("maps");
  if (!mappings.length) {
    host.innerHTML = `<p class="meta">还没有映射。先听一颗键。</p>`;
    $("dock-lead").textContent = "还没有绑定";
    renderKeys($("dock-keys"), []);
    return;
  }
  host.innerHTML = mappings
    .map((m) => {
      const live = m.button === liveButton ? " live" : "";
      return `<article class="map${live}" data-id="${m.id}" data-button="${m.button}">
        <select data-k="button">${Object.entries(BUTTON_LABEL)
          .map(([v, l]) => `<option value="${v}" ${m.button === v ? "selected" : ""}>${l}</option>`)
          .join("")}</select>
        <select data-k="mode">${Object.entries(MODE_LABEL)
          .map(([v, l]) => `<option value="${v}" ${m.mode === v ? "selected" : ""}>${l}</option>`)
          .join("")}</select>
        <button type="button" class="ghost" data-act="record">${prettyKeys(m.keys)
          .map((k) => k)
          .join(" + ") || "录快捷键"}</button>
        <button type="button" class="danger" data-act="del" aria-label="删除">删除</button>
      </article>`;
    })
    .join("");

  const first = mappings[0];
  $("dock-lead").textContent = `${BUTTON_LABEL[first.button] ?? first.button} · ${MODE_LABEL[first.mode]}`;
  renderKeys($("dock-keys"), first.keys);
}

async function persist() {
  await invoke("save_mappings", { mappings });
}

function codeToToken(e: KeyboardEvent): string | null {
  const map: Record<string, string> = {
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
    Tab: "Tab",
    Escape: "Escape",
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
  };
  if (map[e.code]) return map[e.code];
  if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
  if (/^Digit[0-9]$/.test(e.code)) return e.code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(e.code)) return e.code;
  return null;
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
  mappings = snap.config.mappings ?? [];
  applyTheme(snap.config.theme || "dark");
  setPausedUi(!!snap.config.paused);
  $("chk-autostart").toggleAttribute("checked", !!snap.config.autostart);
  ($("chk-autostart") as HTMLInputElement).checked = !!snap.config.autostart;
  $("xmbc-banner").classList.toggle("hidden", !snap.xmbc_running);
  $("cfg-path").textContent = await invoke<string>("config_dir");
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
  });

  await listen<string>("listen-captured", async (ev) => {
    listening = false;
    $("listen-copy").textContent = `听到了：${BUTTON_LABEL[ev.payload] ?? ev.payload}。请录键盘。`;
    $("btn-listen").textContent = "再听一颗";
    document.querySelector(".listen-sheet")?.classList.remove("armed");
    let m = mappings.find((x) => x.button === ev.payload);
    if (!m) {
      m = {
        id: uid(),
        button: ev.payload,
        mode: "hold",
        keys: [],
        label: BUTTON_LABEL[ev.payload] ?? ev.payload,
      };
      mappings.push(m);
      persist();
    }
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
  const willPause = $("btn-pause").textContent === "暂停映射";
  setPausedUi(willPause);
  await invoke("save_paused", { paused: willPause });
});

$("btn-listen").addEventListener("click", async () => {
  listening = true;
  $("listen-copy").textContent = "在听。按鼠标上你要绑定的那颗键，左右键也可以。";
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
  if (sel.dataset.k === "button") m.button = sel.value;
  if (sel.dataset.k === "mode") m.mode = sel.value;
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
    await persist();
    renderMaps();
  }
  if (t.dataset.act === "record") openRecord(id);
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

$("btn-quit")?.addEventListener("click", async () => {
  await invoke("quit_app");
});

boot();

