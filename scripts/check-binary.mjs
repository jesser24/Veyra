// Verifies a built binary's CPU architecture from its header (PE / Mach-O / ELF) and that it starts. usage: node check-binary.mjs <file> <x64|arm64>
import { readFileSync, openSync, readSync, closeSync } from "node:fs";
import { spawnSync } from "node:child_process";
const [file, want] = process.argv.slice(2); const b = Buffer.alloc(4096); const fd = openSync(file, "r"); readSync(fd, b, 0, 4096, 0); closeSync(fd);
let arch = "unknown";
if (b.readUInt16LE(0) === 0x5a4d) { const pe = b.readUInt32LE(0x3c); const m = b.readUInt16LE(pe + 4); arch = m === 0x8664 ? "x64" : m === 0xaa64 ? "arm64" : `pe-0x${m.toString(16)}`; }
else if (b.readUInt32LE(0) === 0xfeedfacf) { const c = b.readUInt32LE(4); arch = c === 0x0100000c ? "arm64" : c === 0x01000007 ? "x64" : `macho-0x${c.toString(16)}`; }
else if (b.readUInt32BE(0) === 0x7f454c46) { const m = b.readUInt16LE(18); arch = m === 0x3e ? "x64" : m === 0xb7 ? "arm64" : `elf-${m}`; }
console.log(`${file}: architecture ${arch}`); if (arch !== want) { console.error(`expected ${want}`); process.exit(1); }
const r = spawnSync(file, ["--version"], { encoding: "utf8", timeout: 30000 }); console.log((r.stdout || "") + (r.stderr || "")); if (r.status !== 0) { console.error("binary did not start (exit " + r.status + ")"); process.exit(1); }
