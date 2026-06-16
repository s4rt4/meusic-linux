// Rasterize the colored logo mark to a 1024x1024 PNG for `tauri icon`.
import sharp from "sharp";
import { readFileSync } from "node:fs";

const svg = readFileSync("assets/logo/logo_icon-color.svg");

await sharp(svg, { density: 512 })
  .resize(1024, 1024, {
    fit: "contain",
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  })
  .png()
  .toFile("app-icon.png");

console.log("Wrote app-icon.png (1024x1024)");
