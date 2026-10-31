// Drives the built site in a real browser. Usage: node site.mjs <dist> <demo.gba> <screenshots>
import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";
import { chromium } from "playwright";

const [dist, romPath, shots] = process.argv.slice(2);
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm" };

const server = createServer(async (request, response) => {
  const requested = new URL(request.url, "http://localhost").pathname;
  const path = requested === "/" ? "/index.html" : requested;
  try {
    const body = await readFile(join(dist, path));
    response.writeHead(200, { "content-type": TYPES[extname(path)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end();
  }
});
await new Promise((resolve) => server.listen(0, resolve));
const url = `http://localhost:${server.address().port}/`;

const browser = await chromium.launch();
const failures = [];

async function test(name, run) {
  try {
    await run();
    console.log(`ok - ${name}`);
  } catch (error) {
    failures.push(name);
    console.log(`FAILED - ${name}: ${error.message}`);
  }
}

await mkdir(shots, { recursive: true });

const openPage = async (options = {}) => {
  const context = await browser.newContext(options);
  const page = await context.newPage();
  page.on("pageerror", (error) => console.log(`page error: ${error.message}`));
  await page.goto(url);
  return page;
};

const centre = (page) =>
  page.evaluate(() => {
    const [r, g, b] = document.getElementById("screen").getContext("2d").getImageData(120, 80, 1, 1).data;
    return { r, g, b };
  });

const waitFor = async (check, what) => {
  const start = Date.now();
  while (!(await check())) {
    if (Date.now() - start > 30000) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
};

const isGreen = async (page) => {
  const { r, g, b } = await centre(page);
  return g > 200 && r < 16 && b < 16;
};
const isRed = async (page) => {
  const { r, g, b } = await centre(page);
  return r > 200 && g < 16 && b < 16;
};
const statusSays = (page, id, text) => async () => (await page.textContent(id)).includes(text);

const page = await openPage({ viewport: { width: 1200, height: 800 } });

await test("the demo plays and reacts to the keyboard", async () => {
  await page.screenshot({ path: join(shots, "start.png") });
  await page.click("#play-demo");
  await waitFor(() => isGreen(page), "a green screen");
  await page.screenshot({ path: join(shots, "playing.png") });
  await page.keyboard.down("KeyZ");
  await waitFor(() => isRed(page), "a red screen");
  await page.keyboard.up("KeyZ");
  await waitFor(() => isGreen(page), "green again");
});

await test("save states round trip", async () => {
  await page.click("#load-state");
  await waitFor(statusSays(page, "#console-status", "no saved state"), "the missing state message");
  await page.click("#save-state");
  await waitFor(statusSays(page, "#console-status", "State saved"), "the save");
  await page.click("#load-state");
  await waitFor(statusSays(page, "#console-status", "State loaded"), "the load");
  await waitFor(() => isGreen(page), "the game to carry on");
});

await test("raw ROMs and renamed ROMs are refused", async () => {
  await page.setInputFiles("#cartridge-input", romPath);
  await waitFor(statusSays(page, "#console-status", "only reads FASTA"), "the extension refusal");
  const rom = await readFile(romPath);
  await page.setInputFiles("#cartridge-input", { name: "game.fasta", mimeType: "text/plain", buffer: rom });
  await waitFor(statusSays(page, "#console-status", "not a FASTA file"), "the content refusal");
  await waitFor(() => isGreen(page), "the running game to be unaffected");
});

await test("a real genome is refused", async () => {
  const genome = Buffer.from(">chr1\nACGTNNNNACGTACGT\n");
  await page.setInputFiles("#cartridge-input", { name: "real.fasta", mimeType: "text/plain", buffer: genome });
  await waitFor(statusSays(page, "#console-status", "does not contain a GBADN"), "the refusal");
});

await test("the converter makes a cartridge that plays", async () => {
  await page.click("#tab-converter");
  await page.setInputFiles("#rom-input", romPath);
  await waitFor(() => page.isVisible("#report"), "the report");
  const report = await page.textContent("#report");
  if (!report.includes("GBADN DEMO") || !report.includes("Cartridge header")) throw new Error(report);
  await page.screenshot({ path: join(shots, "converter.png") });

  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
  if (download.suggestedFilename() !== "gbadn-demo.fasta.gz") throw new Error(download.suggestedFilename());

  await page.click("#play-converted");
  await waitFor(() => isGreen(page), "the converted cartridge");
});

await test("the converter refuses something that isn't a ROM", async () => {
  await page.click("#tab-converter");
  await page.setInputFiles("#rom-input", { name: "notes.gba", mimeType: "text/plain", buffer: Buffer.from("not a game") });
  await waitFor(statusSays(page, "#converter-status", "not a GBA ROM"), "the refusal");
});

await test("touch buttons work on a phone-sized screen", async () => {
  const phone = await openPage({ viewport: { width: 844, height: 390 }, hasTouch: true, isMobile: true });
  await phone.click("#play-demo");
  await waitFor(() => isGreen(phone), "a green screen");
  await phone.screenshot({ path: join(shots, "phone.png") });
  const box = await phone.locator('[data-button="A"]').boundingBox();
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await phone.mouse.move(x, y);
  await phone.mouse.down();
  await waitFor(() => isRed(phone), "a red screen");
  await phone.mouse.up();
  await waitFor(() => isGreen(phone), "green again");
});

await browser.close();
server.close();
if (failures.length) {
  console.log(`${failures.length} failed`);
  process.exit(1);
}
console.log("all passed");
