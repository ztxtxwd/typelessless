#!/usr/bin/env node
// Generate every app icon size + Windows .ico + tray icons from `logo.png`.
//
// Run with: `npm install && node scripts/generate-icons.mjs`.
// Skips icon.icns (macOS-only via `iconutil` — regenerate that on a Mac).

import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdirSync, writeFileSync, existsSync } from "node:fs";
import sharp from "sharp";
import pngToIco from "png-to-ico";

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SOURCE = join(ROOT, "logo.png");
const ICONS_DIR = join(ROOT, "src-tauri", "icons");
const APP_SIZES = [32, 128, 256, 512, 1024];

if (!existsSync(SOURCE)) {
  console.error(`Source logo not found at ${SOURCE}`);
  process.exit(1);
}
mkdirSync(ICONS_DIR, { recursive: true });

async function writePng(size, outName) {
  const out = join(ICONS_DIR, outName);
  await sharp(SOURCE)
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 1 } })
    .png({ compressionLevel: 9 })
    .toFile(out);
  console.log(`  ${outName}`);
}

async function generateAppIcons() {
  console.log("App icons:");
  for (const size of APP_SIZES) {
    await writePng(size, `${size}x${size}.png`);
  }
  // 128x128@2x is just 256x256.
  const src256 = join(ICONS_DIR, "256x256.png");
  const dst = join(ICONS_DIR, "128x128@2x.png");
  writeFileSync(dst, (await sharp(src256).toBuffer()));
  console.log("  128x128@2x.png");
}

async function generateTrayIcons() {
  console.log("Tray icons:");
  // Keep the same black backplate as the app icon — the user wants the badge
  // to read as a tile, not a hollow glyph. (Side-effect: don't enable template
  // mode in tray.rs, otherwise macOS will recolor it.)
  for (const [size, name] of [
    [22, "tray-icon.png"],
    [44, "tray-icon@2x.png"],
  ]) {
    const out = join(ICONS_DIR, name);
    await sharp(SOURCE)
      .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 1 } })
      .png({ compressionLevel: 9 })
      .toFile(out);
    console.log(`  ${name}`);
  }
}

async function generateIco() {
  console.log("icon.ico:");
  const sources = [32, 128, 256].map((s) => join(ICONS_DIR, `${s}x${s}.png`));
  const buf = await pngToIco(sources);
  writeFileSync(join(ICONS_DIR, "icon.ico"), buf);
  console.log("  icon.ico");
}

await generateAppIcons();
await generateTrayIcons();
await generateIco();

if (process.platform === "darwin") {
  console.log("Skipping icon.icns auto-generation; run `iconutil -c icns ...` on a Mac.");
} else {
  console.log("Skipping icon.icns (macOS-only via `iconutil`).");
}

console.log("Done.");
