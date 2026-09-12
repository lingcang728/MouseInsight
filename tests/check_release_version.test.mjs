import assert from "node:assert/strict";
import { readVersions } from "../scripts/check-release-version.mjs";

const { packageVersion, files } = readVersions();

assert.ok(packageVersion, "package.json version must be non-empty");
const entries = Object.entries(files);
assert.equal(entries.length, 6, "six version locations must be checked");
for (const [file, version] of entries) {
  assert.equal(
    version,
    packageVersion,
    `${file} version ${version} != package.json ${packageVersion}`
  );
}
console.log(`check_release_version.test.mjs: all ${entries.length} version locations match ${packageVersion}`);
