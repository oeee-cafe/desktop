// Draws the achievement icons Steamworks asks for (Stats & Achievements):
// 64 by 64, one for an achievement earned and one for not yet, as JPEG.
//
// The glyphs are the Material Symbols the site's profile shows beside each
// achievement (templates/achievement_icon_macro.jinja in oeee-cafe/web),
// with their path data in icons.json beside this file. Earned is the site's
// badge: the glyph in its dark ink on the lime accent. Not yet is the same
// glyph greyed, as Steam lists what is still to be earned.
//
//   node steam/achievements/render.mjs
//
// Needs Playwright's Chromium. PLAYWRIGHT names where to import Playwright
// from when it is not installed here, e.g. oeee-cafe/web's neo-cucumber:
//
//   PLAYWRIGHT=~/Git/oeee-cafe/neo-cucumber/node_modules/playwright/index.mjs \
//     node steam/achievements/render.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const playwright = process.env.PLAYWRIGHT
  ? pathToFileURL(process.env.PLAYWRIGHT.replace(/^~/, process.env.HOME)).href
  : "playwright";
const { chromium } = await import(playwright);

const icons = JSON.parse(readFileSync(join(here, "icons.json"), "utf8"));

// The site's accent and its ink on it (static/ds.css), and a grey pair for
// the achievement not yet earned.
const looks = {
  "": { ground: "#c4ee62", ink: "#1c2811" },
  _locked: { ground: "#8b8b99", ink: "#3b3b46" },
};

const SIZE = 64;
// The glyph at 40 of the 64, as the profile sets 22 in 40: room around it
// for Steam's own frame.
const GLYPH = 40;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: SIZE, height: SIZE } });
for (const [name, { d }] of Object.entries(icons)) {
  for (const [suffix, { ground, ink }] of Object.entries(looks)) {
    const offset = (SIZE - GLYPH) / 2;
    await page.setContent(`<!doctype html>
      <style>html, body { margin: 0; width: ${SIZE}px; height: ${SIZE}px; background: ${ground}; }</style>
      <svg width="${SIZE}" height="${SIZE}" viewBox="0 0 ${SIZE} ${SIZE}" style="display:block">
        <g transform="translate(${offset} ${offset}) scale(${GLYPH / 24})">
          <path fill="${ink}" d="${d}"/>
        </g>
      </svg>`);
    const file = join(here, `${name}${suffix}.jpg`);
    await page.screenshot({ path: file, type: "jpeg", quality: 95 });
    console.log(file);
  }
}
await browser.close();
