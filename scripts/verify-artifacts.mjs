// Fails the build unless the expected installers really exist and are plausibly sized. Writes SHA256SUMS-<label>.txt.
// Usage: node scripts/verify-artifacts.mjs <windows|macos> <label> [outDir]
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync, mkdirSync, copyFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const [platform, label, out = "release-out"] = process.argv.slice(2);
const root = "target/release/bundle";
const MIN = 1_000_000; // an installer under 1 MB is certainly broken
const rules = {
  windows: [{ dir: "nsis", ext: ".exe", what: "NSIS installer" }],
  macos: [{ dir: "dmg", ext: ".dmg", what: "DMG disk image" }],
};
if (!rules[platform]) { console.error(`unknown platform '${platform}'`); process.exit(2); }

const found = [];
let failed = false;
for (const r of rules[platform]) {
  const d = join(root, r.dir);
  const files = existsSync(d) ? readdirSync(d).filter((f) => f.endsWith(r.ext)) : [];
  if (files.length === 0) { console.error(`MISSING: no ${r.what} (*${r.ext}) in ${d}`); failed = true; continue; }
  for (const f of files) {
    const p = join(d, f), size = statSync(p).size;
    if (size < MIN) { console.error(`TOO SMALL: ${p} is ${size} bytes`); failed = true; continue; }
    console.log(`OK: ${p} (${(size / 1048576).toFixed(1)} MB)`); found.push(p);
  }
}
if (platform === "macos") {
  const app = join(root, "macos", "Veyra.app");
  if (!existsSync(join(app, "Contents", "MacOS"))) { console.error(`MISSING: ${app}`); failed = true; } else console.log(`OK: ${app}`);
}
if (failed) process.exit(1);

mkdirSync(out, { recursive: true });
const sums = [];
for (const p of found) {
  const name = p.split("/").pop();
  copyFileSync(p, join(out, name));
  sums.push(`${createHash("sha256").update(readFileSync(p)).digest("hex")}  ${name}`);
}
writeFileSync(join(out, `SHA256SUMS-${label}.txt`), sums.join("\n") + "\n");
console.log(sums.join("\n"));
