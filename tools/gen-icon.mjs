// Generate simple Devil-of-Devin icons (violet rounded square with horn silhouette)
// Pure-node PNG encoder — no deps.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

function crc32(buf) {
  let c, table = [];
  for (let n = 0; n < 256; n++) {
    c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  let crc = 0xffffffff;
  for (const b of buf) crc = table[(crc ^ b) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size, painter) {
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = painter(x / size, y / size);
      const o = y * (size * 4 + 1) + 1 + x * 4;
      raw[o] = r; raw[o + 1] = g; raw[o + 2] = b; raw[o + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0); ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; ihdr[9] = 6; // 8-bit RGBA
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, 9)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// Devil face: violet gradient + two horns + eyes
function paint(u, v) {
  const cx = u - 0.5, cy = v - 0.5;
  const r = Math.hypot(cx, cy);
  // rounded-square mask
  const m = 0.44;
  const d = Math.max(Math.abs(cx), Math.abs(cy));
  const inBox = d < m - 0.02 || (d < m && r < m);
  if (!inBox) return [0, 0, 0, 0];
  // gradient violet
  const t = v;
  const R = Math.round(90 + 40 * t), G = Math.round(60 + 30 * t), B = Math.round(220 + 30 * t);
  // horns (two arcs near top)
  const hornL = Math.hypot(u - 0.30, v - 0.16), hornR = Math.hypot(u - 0.70, v - 0.16);
  if (v < 0.22 && (hornL < 0.10 || hornR < 0.10)) return [240, 235, 255, 255];
  // eyes
  const eyeL = Math.hypot(u - 0.36, v - 0.42), eyeR = Math.hypot(u - 0.64, v - 0.42);
  if (eyeL < 0.055 || eyeR < 0.055) return [20, 10, 40, 255];
  // grin
  const grin = Math.abs(r - 0.26) < 0.03 && v > 0.55 && v < 0.78;
  if (grin) return [20, 10, 40, 255];
  return [R, G, B, 255];
}

mkdirSync("src-tauri/icons", { recursive: true });
for (const [file, size] of [["32x32.png", 32], ["128x128.png", 128], ["128x128@2x.png", 256], ["icon.png", 512]]) {
  writeFileSync(`src-tauri/icons/${file}`, png(size, paint));
  console.log("wrote", file);
}
// ICO = PNG embedded (Tauri accepts png-in-ico for dev) — produce a real .ico containing 32x32 PNG
const png32 = png(32, paint);
const ico = Buffer.concat([
  Buffer.from([0, 0, 1, 0, 1, 0]),
  Buffer.from([32, 32, 0, 0, 1, 0, 32, 0]),
  (() => { const b = Buffer.alloc(4); b.writeUInt32LE(png32.length); return b; })(),
  (() => { const b = Buffer.alloc(4); b.writeUInt32LE(22); return b; })(),
  png32,
]);
writeFileSync("src-tauri/icons/icon.ico", ico);
writeFileSync("src-tauri/icons/icon.icns", png(512, paint)); // dev placeholder
console.log("wrote icon.ico / icon.icns");
