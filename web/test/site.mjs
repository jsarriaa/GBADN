// Drives the built site in a real browser. Usage: node site.mjs <dist> <demo.gba> <screenshots>
import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";
import { chromium } from "playwright";

const [dist, romPath, shots] = process.argv.slice(2);
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".svg": "image/svg+xml", ".png": "image/png" };

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

// What a person sees, which the canvas's own pixels can't tell.
const expectGameOnScreen = async (target) => {
  const { x, y, width, height } = await target.locator("#screen").boundingBox();
  const middle = await target.evaluate(
    ([px, py]) => document.elementFromPoint(px, py)?.id,
    [x + width / 2, y + height / 2],
  );
  if (middle !== "screen") throw new Error(`${middle} covers the game`);
  const viewport = target.viewportSize();
  if (x < 0 || y < 0 || x + width > viewport.width + 1 || y + height > viewport.height + 1) {
    throw new Error(`the game runs off the page: ${JSON.stringify({ x, y, width, height })}`);
  }
};

await test("the start screen only offers what makes sense", async () => {
  for (const id of ["#close-game", "#save-state", "#load-state"]) {
    if (await page.isVisible(id)) throw new Error(`${id} is showing with no game`);
  }
  if (!(await page.isVisible("#drop-hint"))) throw new Error("the prompt is missing");
  await page.screenshot({ path: join(shots, "start.png") });
});

await test("the demo plays and reacts to the keyboard", async () => {
  await page.click("#play-demo");
  await waitFor(() => isGreen(page), "a green screen");
  await expectGameOnScreen(page);
  if (await page.isVisible("#drop-hint")) throw new Error("the prompt is still showing");
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
  await expectGameOnScreen(phone);
  await phone.screenshot({ path: join(shots, "phone.png") });
  for (const button of ["UP", "DOWN", "LEFT", "RIGHT", "A", "B", "L", "R", "START", "SELECT"]) {
    const { x, y, width, height } = await phone.locator(`[data-button="${button}"]`).boundingBox();
    const viewport = phone.viewportSize();
    if (x < 0 || y < 0 || x + width > viewport.width + 1 || y + height > viewport.height + 1) {
      throw new Error(`${button} is off the screen`);
    }
  }
  const box = await phone.locator('[data-button="A"]').boundingBox();
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await phone.mouse.move(x, y);
  await phone.mouse.down();
  await waitFor(() => isRed(phone), "a red screen");
  await phone.mouse.up();
  await waitFor(() => isGreen(phone), "green again");
});

await test("a phone held upright fits too", async () => {
  const phone = await openPage({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  await phone.click("#play-demo");
  await waitFor(() => isGreen(phone), "a green screen");
  await expectGameOnScreen(phone);
  await phone.screenshot({ path: join(shots, "phone-upright.png") });
});

await test("the genome study shows every figure and fits a phone", async () => {
  for (const [name, width] of [["study", 1200], ["study-phone", 400]]) {
    const context = await browser.newContext({ viewport: { width, height: 800 } });
    const study = await context.newPage();
    await study.goto(`${url}index.html`);
    await study.click(".study-link");
    await study.waitForURL("**/study.html");
    await study.evaluate(() => document.querySelectorAll("img").forEach((image) => { image.loading = "eager"; }));
    const figures = await study.locator("main > figure img").count();
    if (figures !== 5) throw new Error(`${figures} figures`);
    if (!(await study.locator(".twin").isVisible())) throw new Error("the genome twin is missing");
    if ((await study.locator(".pair img").count()) !== 2) throw new Error("the side-by-side pictures are missing");
    await waitFor(
      () => study.evaluate(() => [...document.images].filter((image) => image.src.startsWith(location.origin)).every((image) => image.complete && image.naturalWidth > 0)),
      "the figures to load",
    );
    const overflow = await study.evaluate(() => document.documentElement.scrollWidth - innerWidth);
    if (overflow > 0) throw new Error(`${overflow}px wider than a ${width}px screen`);
    await study.screenshot({ path: join(shots, `${name}.png`), fullPage: true });
    await context.close();
  }
});

await test("the logo renders", async () => {
  const context = await browser.newContext({ viewport: { width: 820, height: 540 } });
  const logo = await context.newPage();
  const response = await logo.goto(`${url}logo.svg`);
  if (!response.ok()) throw new Error(`logo.svg: ${response.status()}`);
  await logo.screenshot({ path: join(shots, "logo.png") });
  await logo.goto(`${url}favicon.svg`);
  await logo.screenshot({ path: join(shots, "favicon.png") });
  const small = await browser.newContext({ viewport: { width: 192, height: 192 } });
  const icon = await small.newPage();
  await icon.setContent(
    `<body style="margin:0;background:#16181C;display:flex;align-items:center;justify-content:center;height:192px"><img src="${url}logo.svg" width="142"></body>`,
    { waitUntil: "networkidle" },
  );
  await icon.screenshot({ path: join(shots, "icon-size.png") });
});

await browser.close();
server.close();
if (failures.length) {
  console.log(`${failures.length} failed`);
  process.exit(1);
}
console.log("all passed");
