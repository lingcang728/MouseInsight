export const RELEASES_URL = "https://github.com/lingcang728/MouseInsight/releases";
const API_URL = "https://api.github.com/repos/lingcang728/MouseInsight/releases/latest";

function parseVersion(value: string) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(value);
  return match ? { core: match.slice(1, 4).map(Number), pre: match[4]?.split(".") } : null;
}

export function isNewer(remote: string, current: string): boolean {
  const a = parseVersion(remote), b = parseVersion(current);
  if (!a || !b) return false;
  for (let i = 0; i < 3; i++) {
    if (a.core[i] !== b.core[i]) return a.core[i] > b.core[i];
  }
  if (!a.pre || !b.pre) return !a.pre && !!b.pre;
  for (let i = 0; i < Math.max(a.pre.length, b.pre.length); i++) {
    const x = a.pre[i], y = b.pre[i];
    if (x === y) continue;
    if (x === undefined || y === undefined) return y === undefined;
    const nx = /^\d+$/.test(x), ny = /^\d+$/.test(y);
    if (nx && ny) return Number(x) > Number(y);
    if (nx !== ny) return !nx;
    return x > y;
  }
  return false;
}

type Release = { version: string; url: string };
let cached: { release: Release; expires: number } | undefined;
let pending: Promise<Release> | undefined;

/** One bounded request; cache only successful metadata, never failed attempts. */
export function latestRelease(): Promise<Release> {
  if (cached && Date.now() < cached.expires) return Promise.resolve(cached.release);
  if (pending) return pending;
  pending = (async () => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 8000);
    try {
      const response = await fetch(API_URL, { signal: controller.signal, headers: { Accept: "application/vnd.github+json" } });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const data = await response.json();
      if (typeof data.tag_name !== "string" || !parseVersion(data.tag_name)) throw new Error("无效版本信息");
      const release = { version: data.tag_name, url: `${RELEASES_URL}/latest` };
      cached = { release, expires: Date.now() + 5 * 60_000 };
      return release;
    } finally {
      clearTimeout(timer);
    }
  })().finally(() => { pending = undefined; });
  return pending;
}
