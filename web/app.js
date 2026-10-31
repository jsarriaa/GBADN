"use strict";

const WIDTH = 240;
const HEIGHT = 160;
const FRAME_MS = 280896 / 16777.216;
const MAX_FRAMES_PER_PAINT = 3;
const SAVE_INTERVAL_MS = 2000;
const AUDIO_FRAMES = 2048;
const ACCEPTED_SUFFIXES = [".fasta", ".fa", ".fna", ".fasta.gz", ".fa.gz", ".fna.gz"];

const BUTTONS = { A: 1, B: 2, SELECT: 4, START: 8, RIGHT: 16, LEFT: 32, UP: 64, DOWN: 128, R: 256, L: 512 };
const KEYMAP = {
  KeyZ: BUTTONS.A,
  KeyX: BUTTONS.B,
  KeyA: BUTTONS.L,
  KeyS: BUTTONS.R,
  Enter: BUTTONS.START,
  Backspace: BUTTONS.SELECT,
  ArrowUp: BUTTONS.UP,
  ArrowDown: BUTTONS.DOWN,
  ArrowLeft: BUTTONS.LEFT,
  ArrowRight: BUTTONS.RIGHT,
};

const $ = (id) => document.getElementById(id);

// ---- Codec worker ----

const codec = new Worker("codec-worker.js");
const pendingCodecCalls = new Map();
let nextCodecCall = 0;

codec.onmessage = ({ data }) => {
  const { resolve, reject } = pendingCodecCalls.get(data.id);
  pendingCodecCalls.delete(data.id);
  if (data.error) reject(new Error(data.error));
  else resolve(data);
};

function callCodec(action, bytes) {
  return new Promise((resolve, reject) => {
    const id = nextCodecCall++;
    pendingCodecCalls.set(id, { resolve, reject });
    codec.postMessage({ id, action, bytes });
  });
}

// ---- Saved games, kept in this browser ----

const database = new Promise((resolve, reject) => {
  const request = indexedDB.open("gbadn", 1);
  request.onupgradeneeded = () => request.result.createObjectStore("files");
  request.onsuccess = () => resolve(request.result);
  request.onerror = () => reject(request.error);
});

async function storedFile(key) {
  const db = await database;
  return new Promise((resolve, reject) => {
    const request = db.transaction("files").objectStore("files").get(key);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function storeFile(key, bytes) {
  const db = await database;
  return new Promise((resolve, reject) => {
    const transaction = db.transaction("files", "readwrite");
    transaction.objectStore("files").put(bytes, key);
    transaction.oncomplete = resolve;
    transaction.onerror = () => reject(transaction.error);
  });
}

async function fingerprint(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

// ---- Console ----

const canvas = $("screen");
const context = canvas.getContext("2d");
const image = context.createImageData(WIDTH, HEIGHT);
const coreModule = createGbadn();

let game = null;
let keyboardButtons = 0;
const touchButtons = new Map();

function setConsoleStatus(message, isError = false) {
  const status = $("console-status");
  status.textContent = message;
  status.classList.toggle("is-error", isError);
}

function isAcceptedName(name) {
  const lower = name.toLowerCase();
  return ACCEPTED_SUFFIXES.some((suffix) => lower.endsWith(suffix));
}

async function openCartridge(name, bytes) {
  if (!isAcceptedName(name)) {
    setConsoleStatus("This console only reads FASTA genomic files (.fasta, .fa, .fna, optionally .gz)", true);
    return;
  }
  setConsoleStatus("Reading the sequence…");
  startSound();
  try {
    const { bytes: rom, message: title } = await callCodec("decode", bytes);
    await startGame(rom, title);
    setConsoleStatus("");
  } catch (error) {
    setConsoleStatus(error.message, true);
  }
}

async function startGame(rom, title) {
  const gbadn = await coreModule;
  const key = await fingerprint(rom);
  closeGame();

  const statusPointer = gbadn._malloc(4);
  const romPointer = gbadn._malloc(rom.length);
  gbadn.HEAPU8.set(rom, romPointer);
  const core = gbadn._gbadn_core_new(romPointer, rom.length, 0, statusPointer);
  gbadn._free(romPointer);
  gbadn._free(statusPointer);
  if (!core) {
    throw new Error("The sequence decoded correctly, but it is not a GBA program");
  }

  game = {
    gbadn,
    core,
    key,
    title,
    video: gbadn._gbadn_core_video(core),
    audioPointer: gbadn._malloc(AUDIO_FRAMES * 4),
    nextFrame: performance.now(),
    lastSave: null,
    lastSaveCheck: performance.now(),
  };

  const save = await storedFile(`save:${key}`);
  if (save) {
    const pointer = gbadn._malloc(save.length);
    gbadn.HEAPU8.set(save, pointer);
    gbadn._gbadn_core_savedata_restore(core, pointer, save.length);
    gbadn._free(pointer);
    game.lastSave = save;
  }

  $("game-title").textContent = title;
  $("drop-hint").hidden = true;
  document.body.classList.add("is-playing");
  for (const id of ["close-game", "save-state", "load-state"]) $(id).hidden = false;
  requestAnimationFrame(tick);
}

function closeGame() {
  if (!game) return;
  storeSaveIfChanged();
  game.gbadn._free(game.audioPointer);
  game.gbadn._gbadn_core_free(game.core);
  game = null;
  context.clearRect(0, 0, WIDTH, HEIGHT);
  $("game-title").textContent = "";
  $("drop-hint").hidden = false;
  document.body.classList.remove("is-playing");
  for (const id of ["close-game", "save-state", "load-state"]) $(id).hidden = true;
}

function tick(now) {
  if (!game) return;
  let frames = 0;
  while (game.nextFrame <= now && frames < MAX_FRAMES_PER_PAINT) {
    const held = [...touchButtons.values()].reduce((mask, button) => mask | button, keyboardButtons);
    game.gbadn._gbadn_core_run_frame(game.core, held);
    playSound();
    game.nextFrame += FRAME_MS;
    frames += 1;
  }
  if (game.nextFrame <= now) game.nextFrame = now + FRAME_MS;

  if (frames > 0) {
    const pixels = game.gbadn.HEAPU8.subarray(game.video, game.video + WIDTH * HEIGHT * 4);
    image.data.set(pixels);
    for (let alpha = 3; alpha < image.data.length; alpha += 4) image.data[alpha] = 255;
    context.putImageData(image, 0, 0);
  }

  if (now - game.lastSaveCheck >= SAVE_INTERVAL_MS) {
    game.lastSaveCheck = now;
    storeSaveIfChanged();
  }
  requestAnimationFrame(tick);
}

function storeSaveIfChanged() {
  if (!game) return;
  const { gbadn, core, key } = game;
  const pointerPointer = gbadn._malloc(4);
  const size = gbadn._gbadn_core_savedata_clone(core, pointerPointer);
  const pointer = gbadn.HEAPU32[pointerPointer >> 2];
  gbadn._free(pointerPointer);
  if (!size) return;
  const save = gbadn.HEAPU8.slice(pointer, pointer + size);
  gbadn._free(pointer);
  const previous = game.lastSave;
  if (previous && previous.length === save.length && previous.every((byte, index) => byte === save[index])) return;
  game.lastSave = save;
  storeFile(`save:${key}`, save).catch((error) => setConsoleStatus(`Could not store the save: ${error.message}`, true));
}

async function saveState() {
  if (!game) return;
  const { gbadn, core, key } = game;
  const size = gbadn._gbadn_core_state_size(core);
  const pointer = gbadn._malloc(size);
  const saved = gbadn._gbadn_core_save_state(core, pointer);
  const state = gbadn.HEAPU8.slice(pointer, pointer + size);
  gbadn._free(pointer);
  if (!saved) {
    setConsoleStatus("Could not save the state", true);
    return;
  }
  await storeFile(`state:${key}`, state);
  setConsoleStatus("State saved");
}

async function loadState() {
  if (!game) return;
  const { gbadn, core, key } = game;
  const state = await storedFile(`state:${key}`);
  if (!state) {
    setConsoleStatus("There is no saved state for this game yet", true);
    return;
  }
  const pointer = gbadn._malloc(state.length);
  gbadn.HEAPU8.set(state, pointer);
  const loaded = state.length === gbadn._gbadn_core_state_size(core) && gbadn._gbadn_core_load_state(core, pointer);
  gbadn._free(pointer);
  setConsoleStatus(loaded ? "State loaded" : "Could not load the state", !loaded);
}

// ---- Sound ----

let audioContext = null;
let soundNode = null;

// Browsers only allow sound after a click or key press, which opening a file always follows.
async function startSound() {
  if (audioContext) {
    audioContext.resume();
    return;
  }
  try {
    audioContext = new AudioContext({ latencyHint: "interactive" });
    await audioContext.audioWorklet.addModule("sound-worklet.js");
    soundNode = new AudioWorkletNode(audioContext, "sound-player", { outputChannelCount: [2] });
    soundNode.connect(audioContext.destination);
    audioContext.resume();
  } catch {
    // A machine without sound can still play.
    soundNode = null;
  }
}

function playSound() {
  const { gbadn, core, audioPointer } = game;
  const rate = audioContext ? audioContext.sampleRate : 48000;
  const frames = gbadn._gbadn_core_read_audio(core, audioPointer, AUDIO_FRAMES, rate);
  if (soundNode && frames > 0) {
    const samples = gbadn.HEAP16.slice(audioPointer >> 1, (audioPointer >> 1) + frames * 2);
    soundNode.port.postMessage(samples, [samples.buffer]);
  }
}

// ---- Input ----

addEventListener("keydown", (event) => {
  if (!game || event.ctrlKey || event.metaKey || event.altKey) return;
  if (event.code === "F2" || event.code === "F4") {
    event.preventDefault();
    if (!event.repeat) (event.code === "F2" ? saveState : loadState)();
    return;
  }
  const button = KEYMAP[event.code];
  if (button) {
    event.preventDefault();
    keyboardButtons |= button;
  }
});

addEventListener("keyup", (event) => {
  const button = KEYMAP[event.code];
  if (button) keyboardButtons &= ~button;
});

addEventListener("blur", () => {
  keyboardButtons = 0;
  touchButtons.clear();
  paintTouchButtons();
});

function buttonUnder(event) {
  const element = document.elementFromPoint(event.clientX, event.clientY);
  return element && element.dataset ? BUTTONS[element.dataset.button] : undefined;
}

function paintTouchButtons() {
  const held = new Set(touchButtons.values());
  for (const element of document.querySelectorAll("[data-button]")) {
    element.classList.toggle("is-held", held.has(BUTTONS[element.dataset.button]));
  }
}

// Fingers may slide from button to button, so follow each one over the whole page.
addEventListener("pointerdown", (event) => {
  const button = buttonUnder(event);
  if (button) {
    event.preventDefault();
    touchButtons.set(event.pointerId, button);
    paintTouchButtons();
  }
});

addEventListener("pointermove", (event) => {
  if (!touchButtons.has(event.pointerId)) return;
  const button = buttonUnder(event);
  if (button) touchButtons.set(event.pointerId, button);
  else touchButtons.delete(event.pointerId);
  paintTouchButtons();
});

for (const type of ["pointerup", "pointercancel"]) {
  addEventListener(type, (event) => {
    touchButtons.delete(event.pointerId);
    paintTouchButtons();
  });
}

for (const element of document.querySelectorAll("[data-button]")) {
  element.addEventListener("contextmenu", (event) => event.preventDefault());
}

if (matchMedia("(pointer: coarse)").matches) document.body.classList.add("has-touch");

addEventListener("pagehide", storeSaveIfChanged);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) storeSaveIfChanged();
  else if (game) game.nextFrame = performance.now();
});

// ---- Files ----

async function readFile(file) {
  return new Uint8Array(await file.arrayBuffer());
}

$("cartridge-input").addEventListener("change", async (event) => {
  const [file] = event.target.files;
  event.target.value = "";
  if (file) openCartridge(file.name, await readFile(file));
});

$("play-demo").addEventListener("click", async () => {
  const response = await fetch("demo.fasta");
  openCartridge("demo.fasta", new Uint8Array(await response.arrayBuffer()));
});

$("close-game").addEventListener("click", () => {
  closeGame();
  setConsoleStatus("");
});
$("save-state").addEventListener("click", saveState);
$("load-state").addEventListener("click", loadState);

function acceptDrops(area, onFile) {
  for (const type of ["dragenter", "dragover"]) {
    area.addEventListener(type, (event) => {
      event.preventDefault();
      area.classList.add("is-dropping");
    });
  }
  area.addEventListener("dragleave", () => area.classList.remove("is-dropping"));
  area.addEventListener("drop", async (event) => {
    event.preventDefault();
    area.classList.remove("is-dropping");
    const [file] = event.dataTransfer.files;
    if (file) onFile(file);
  });
}

acceptDrops($("screen-area"), async (file) => openCartridge(file.name, await readFile(file)));

// ---- Tabs ----

function showTab(name) {
  for (const section of ["console", "converter"]) {
    $(section).hidden = section !== name;
    $(`tab-${section}`).setAttribute("aria-selected", String(section === name));
  }
}

$("tab-console").addEventListener("click", () => showTab("console"));
$("tab-converter").addEventListener("click", () => showTab("converter"));

// ---- Converter ----

let converted = null;

function formatBytes(bytes) {
  return bytes >= 1048576 ? `${(bytes / 1048576).toFixed(2)} MiB` : `${(bytes / 1024).toFixed(1)} KiB`;
}

function fillTable(table, rows) {
  table.replaceChildren(...rows.map(([name, value]) => {
    const row = document.createElement("tr");
    for (const text of [name, value]) {
      const cell = document.createElement("td");
      cell.textContent = text;
      row.append(cell);
    }
    return row;
  }));
}

function setConverterStatus(message, isError = false) {
  const status = $("converter-status");
  status.textContent = message;
  status.classList.toggle("is-error", isError);
}

async function convert(file) {
  $("report").hidden = true;
  converted = null;
  setConverterStatus(`Sequencing ${file.name}. A large game can take a minute or more…`);
  try {
    const { bytes, message } = await callCodec("encode", await readFile(file));
    const report = JSON.parse(message);
    const stem = file.name.replace(/\.[^.]*$/, "");
    converted = { name: `${stem}.fasta.gz`, bytes };
    $("report-title").textContent = report.code ? `${report.title} (${report.code})` : report.title;
    fillTable($("report-rows"), [
      ["ROM", formatBytes(report.romBytes)],
      ["Cartridge (.fasta.gz)", formatBytes(report.gzipBytes)],
      ["Uncompressed FASTA", formatBytes(report.fastaBytes)],
      ["Sequence", `${report.bases.toLocaleString("en")} bases, ${(report.gc * 100).toFixed(1)}% GC`],
      ["MD5", report.md5],
    ]);
    fillTable($("report-chromosomes"), report.chromosomes.map((chromosome) => [
      chromosome.description,
      `${formatBytes(chromosome.romBytes)}, ${((chromosome.romBytes * 100) / report.romBytes).toFixed(1)}%`,
    ]));
    $("report").hidden = false;
    setConverterStatus("");
  } catch (error) {
    setConverterStatus(error.message, true);
  }
}

$("rom-input").addEventListener("change", (event) => {
  const [file] = event.target.files;
  event.target.value = "";
  if (file) convert(file);
});
acceptDrops($("rom-drop"), convert);

$("download").addEventListener("click", () => {
  const url = URL.createObjectURL(new Blob([converted.bytes], { type: "application/gzip" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = converted.name;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
});

$("play-converted").addEventListener("click", () => {
  showTab("console");
  openCartridge(converted.name, converted.bytes.slice());
});
