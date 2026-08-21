// Rebuild index.html, then screenshot it.
//
//   node shoot.mjs --out shots/hero.png --sel "#masthead"
//   node shoot.mjs --out shots/page.png --full
//   node shoot.mjs --out shots/mobile.png --width 420
//
// Prints the absolute path of the PNG it wrote.
import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const ROOT = path.dirname(fileURLToPath(import.meta.url));

const argv = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = argv.indexOf('--' + name);
  return i === -1 ? fallback : argv[i + 1];
};
const flag = (name) => argv.includes('--' + name);

const out = path.resolve(ROOT, arg('out', 'shots/page.png'));
const width = parseInt(arg('width', '1440'), 10);
const height = parseInt(arg('height', '900'), 10);
const sel = arg('sel', null);
const pageFile = arg('page', 'index.html');

execFileSync('python3', [path.join(ROOT, 'build.py')], { stdio: 'inherit' });
fs.mkdirSync(path.dirname(out), { recursive: true });

const browser = await chromium.launch({ channel: 'chrome' });
const page = await browser.newPage({
  viewport: { width, height },
  deviceScaleFactor: 2,
});
await page.goto(pathToFileURL(path.join(ROOT, pageFile)).href, {
  waitUntil: 'networkidle',
});
await page.evaluate(() => document.fonts && document.fonts.ready);
// let scroll-reveal / animation settle
await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
await page.waitForTimeout(700);
await page.evaluate(() => window.scrollTo(0, 0));
await page.waitForTimeout(900);

if (sel) {
  // Clip from a VIEWPORT capture. Two capture paths were tried and both
  // silently dropped the animated, absolutely-positioned hero mascot:
  // elementHandle.screenshot(), and page.screenshot({fullPage:true, clip}).
  // Only a plain viewport screenshot renders it. Anything judging design
  // from these images has to see what the browser actually paints.
  await page.evaluate((s) => {
    const el = document.querySelector(s);
    if (el) el.scrollIntoView({ block: 'center', behavior: 'instant' });
  }, sel);
  await page.waitForTimeout(450);

  const box = await page.evaluate((s) => {
    const el = document.querySelector(s);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return {
      x: Math.max(0, r.left),
      y: Math.max(0, r.top),
      width: Math.min(r.width, window.innerWidth - Math.max(0, r.left)),
      height: Math.min(r.height, window.innerHeight - Math.max(0, r.top)),
    };
  }, sel);
  if (!box) {
    console.error('selector not found: ' + sel);
    process.exit(2);
  }
  if (box.height < 20) {
    console.error('element taller than the viewport at this size; raise --height');
  }
  await page.screenshot({ path: out, clip: box });
} else {
  await page.screenshot({ path: out, fullPage: flag('full') });
}

await browser.close();
console.log(out);
