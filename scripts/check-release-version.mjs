import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function readJson(rel) {
  return JSON.parse(readFileSync(join(root, rel), "utf8"));
}

function cargoPackageVersion(text) {
  const match = text.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("could not find [package] version in src-tauri/Cargo.toml");
  }
  return match[1];
}

const packageVersion = readJson("package.json").version;
const tauriVersion = readJson("src-tauri/tauri.conf.json").version;
const cargoVersion = cargoPackageVersion(
  readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8")
);

const files = {
  "package.json": packageVersion,
  "src-tauri/Cargo.toml": cargoVersion,
  "src-tauri/tauri.conf.json": tauriVersion,
};

const mismatched = Object.entries(files).filter(([, version]) => version !== packageVersion);
if (mismatched.length) {
  console.error("Version files are out of sync:");
  for (const [file, version] of Object.entries(files)) {
    console.error(`  ${file}: ${version}`);
  }
  process.exit(1);
}

const rawTag = (process.argv[2] || process.env.GITHUB_REF_NAME || "")
  .trim()
  .replace(/^refs\/tags\//, "");

if (rawTag) {
  if (!rawTag.startsWith("v")) {
    console.error(`Tag must start with v, got: ${rawTag}`);
    process.exit(1);
  }
  const tagVersion = rawTag.slice(1);
  if (tagVersion !== packageVersion) {
    console.error(
      `Tag ${rawTag} does not match application version ${packageVersion}`
    );
    process.exit(1);
  }
  console.log(
    `OK: ${rawTag} matches package.json / Cargo.toml / tauri.conf.json (${packageVersion})`
  );
} else {
  console.log(
    `OK: package.json / Cargo.toml / tauri.conf.json all ${packageVersion}`
  );
}
