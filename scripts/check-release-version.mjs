import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function readJson(rel) {
  return JSON.parse(readFileSync(join(root, rel), "utf8"));
}

function cargoPackageVersion(text) {
  const match = text.match(/\[package\][\s\S]*?^\s*version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("could not find [package] version in src-tauri/Cargo.toml");
  }
  return match[1];
}

export function readVersions() {
  const packageVersion = readJson("package.json").version;
  const tauriVersion = readJson("src-tauri/tauri.conf.json").version;
  const cargoVersion = cargoPackageVersion(
    readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8")
  );

  const npmLock = readJson("package-lock.json");
  const rustLock = readFileSync(join(root, "src-tauri/Cargo.lock"), "utf8");
  const cargoLockVersion = rustLock.match(/name = "mouse_insight"\r?\nversion = "([^"]+)"/)?.[1];
  const files = {
    "package.json": packageVersion,
    "src-tauri/Cargo.toml": cargoVersion,
    "src-tauri/tauri.conf.json": tauriVersion,
    "package-lock.json": npmLock.version,
    "package-lock.json root package": npmLock.packages?.[""]?.version ?? npmLock.version,
    "src-tauri/Cargo.lock": cargoLockVersion,
  };
  return { packageVersion, files };
}

const ciTag = process.env.GITHUB_REF?.startsWith("refs/tags/")
  ? process.env.GITHUB_REF_NAME
  : "";

export function main(rawArg = process.argv[2] || ciTag) {
  const { packageVersion, files } = readVersions();
  const mismatched = Object.entries(files).filter(([, version]) => version !== packageVersion);
  if (mismatched.length) {
    console.error("Version files are out of sync:");
    for (const [file, version] of Object.entries(files)) {
      console.error(`  ${file}: ${version}`);
    }
    process.exit(1);
  }

  const rawTag = rawArg.trim().replace(/^refs\/tags\//, "");

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
      `OK: all version manifests and lockfiles match ${packageVersion}`
    );
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
