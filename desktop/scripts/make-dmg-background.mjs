// Renders assets/dmg/background.html to background.png (660x400) and background@2x.png, which
// dmg-macos.sh joins into one high-resolution image. Needs puppeteer-core and Chrome:
//   node scripts/make-dmg-background.mjs
import puppeteer from "puppeteer-core";
import { fileURLToPath } from "node:url";
import path from "node:path";

const dir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../assets/dmg");
const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const browser = await puppeteer.launch({ executablePath: chrome, headless: true });
for (const [scale, name] of [[1, "background.png"], [2, "background@2x.png"]]) {
  const page = await browser.newPage();
  await page.setViewport({ width: 660, height: 400, deviceScaleFactor: scale });
  await page.goto(`file://${dir}/background.html`);
  await page.screenshot({ path: path.join(dir, name) });
}
await browser.close();
