#!/usr/bin/env node
// Black-box ProcessTerminal lifecycle/protocol traces from pinned pi-tui.
// All process streams, signals, clocks, and timers are replaced with fakes;
// this script never changes the controlling terminal.
// PI_TUI_DIST=<pi-tui dist> node tools/golden/gen-golden-process-terminal.mjs
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}

const terminalPath = join(dist, "terminal.js");
const terminal = await import(pathToFileURL(terminalPath).href);
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const digest = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const saved = {
  stdin: {}, stdout: {}, kill: process.kill,
  setTimeout: globalThis.setTimeout, clearTimeout: globalThis.clearTimeout,
  setInterval: globalThis.setInterval, clearInterval: globalThis.clearInterval,
  dateNow: Date.now,
  columns: Object.getOwnPropertyDescriptor(process.stdout, "columns"),
  rows: Object.getOwnPropertyDescriptor(process.stdout, "rows"),
  isRaw: Object.getOwnPropertyDescriptor(process.stdin, "isRaw"),
  platform: Object.getOwnPropertyDescriptor(process, "platform"),
  envColumns: process.env.COLUMNS, envLines: process.env.LINES,
};
for (const key of ["setRawMode", "setEncoding", "resume", "pause", "on", "removeListener"]) {
  saved.stdin[key] = process.stdin[key];
}
for (const key of ["write", "on", "removeListener"]) saved.stdout[key] = process.stdout[key];

let operations = [];
let stdinListeners = [];
let resizeListeners = [];
let raw = false;
let columns = 80;
let rows = 24;
let now = 0;
let nextTimerId = 1;
const timers = new Map();

function display(data) {
  return JSON.stringify(data).slice(1, -1);
}
function reset({ wasRaw = false, width = 80, height = 24 } = {}) {
  operations = [];
  stdinListeners = [];
  resizeListeners = [];
  raw = wasRaw;
  columns = width;
  rows = height;
  now = 0;
  timers.clear();
  nextTimerId = 1;
}
function schedule(callback, delay, interval) {
  const id = nextTimerId++;
  timers.set(id, { callback, due: now + Number(delay || 0), interval });
  return id;
}
function clearTimer(id) { timers.delete(id); }
function nextDue() {
  return [...timers.values()].reduce((min, timer) => Math.min(min, timer.due), Infinity);
}
function advanceTo(target) {
  while (true) {
    let selectedId;
    let selected;
    for (const [id, timer] of timers) {
      if (timer.due <= target && (!selected || timer.due < selected.due || (timer.due === selected.due && id < selectedId))) {
        selectedId = id;
        selected = timer;
      }
    }
    if (!selected) break;
    now = selected.due;
    if (selected.interval === undefined) timers.delete(selectedId);
    else selected.due += selected.interval;
    selected.callback();
  }
  now = target;
}
function advance(ms) { advanceTo(now + ms); }
function emitInput(data) {
  for (const listener of [...stdinListeners]) listener(data);
}
function emitResize() {
  for (const listener of [...resizeListeners]) listener();
}

process.stdin.setRawMode = (value) => { raw = value; operations.push(`stdin.setRawMode:${value}`); };
process.stdin.setEncoding = (value) => operations.push(`stdin.setEncoding:${value}`);
process.stdin.resume = () => operations.push("stdin.resume");
process.stdin.pause = () => operations.push("stdin.pause");
process.stdin.on = (event, listener) => {
  operations.push(`stdin.on:${event}`);
  if (event === "data") stdinListeners.push(listener);
  return process.stdin;
};
process.stdin.removeListener = (event, listener) => {
  operations.push(`stdin.removeListener:${event}`);
  if (event === "data") stdinListeners = stdinListeners.filter((candidate) => candidate !== listener);
  return process.stdin;
};
process.stdout.write = (data) => { operations.push(`stdout.write:${display(String(data))}`); return true; };
process.stdout.on = (event, listener) => {
  operations.push(`stdout.on:${event}`);
  if (event === "resize") resizeListeners.push(listener);
  return process.stdout;
};
process.stdout.removeListener = (event, listener) => {
  operations.push(`stdout.removeListener:${event}`);
  if (event === "resize") resizeListeners = resizeListeners.filter((candidate) => candidate !== listener);
  return process.stdout;
};
process.kill = (_pid, signal) => { operations.push(`process.kill:${signal}`); return true; };
Object.defineProperty(process.stdin, "isRaw", { configurable: true, get: () => raw });
Object.defineProperty(process.stdout, "columns", { configurable: true, get: () => columns });
Object.defineProperty(process.stdout, "rows", { configurable: true, get: () => rows });
globalThis.setTimeout = (callback, delay) => schedule(callback, delay, undefined);
globalThis.clearTimeout = clearTimer;
globalThis.setInterval = (callback, delay) => schedule(callback, delay, Number(delay || 0));
globalThis.clearInterval = clearTimer;
Date.now = () => now;

function state(instance) {
  return {
    kittyProtocolActive: instance.kittyProtocolActive,
    modifyOtherKeysActive: instance.modifyOtherKeysActive,
    raw,
    listenerCount: stdinListeners.length,
    resizeListenerCount: resizeListeners.length,
  };
}

const out = {
  oracle: {
    package: pkg.name,
    version: pkg.version,
    files: { "terminal.js": digest(terminalPath) },
    platform: process.platform,
  },
  constants: {},
  normalizers: [],
  startupKittyStop: {},
  fallbackThenKitty: {},
  splitNegotiation: {},
  negotiationTimeout: {},
  coarseTick: {},
  drain: {},
  progress: {},
  geometryAndWrites: {},
  restoreRaw: {},
  windowsStart: {},
};

for (const sequence of [
  "\x1b[?7u", "\x1b[?0u", "\x1b[?1;2c", "\x1b[?c", "\x1b[?u", "\x1b[c", "hello",
  "\r",
]) {
  out.normalizers.push({
    sequence,
    negotiation: terminal.parseKeyboardProtocolNegotiationSequence(sequence) ?? null,
    nativeFalse: terminal.normalizeNativeShiftEnterInput(sequence, false, true),
    nativeNoShift: terminal.normalizeNativeShiftEnterInput(sequence, true, false),
    nativeShift: terminal.normalizeNativeShiftEnterInput(sequence, true, true),
    appleShift: terminal.normalizeAppleTerminalInput(sequence, true, true),
  });
}

reset();
{
  const input = [];
  let resizeCount = 0;
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push(data), () => { resizeCount += 1; });
  const afterStart = { operations: [...operations], state: state(instance) };
  emitResize();
  emitInput("\x1b[?7u");
  emitInput("a");
  const afterInput = { operations: [...operations], input: [...input], resizeCount, state: state(instance) };
  instance.stop();
  out.startupKittyStop = { afterStart, afterInput, afterStop: { operations: [...operations], input, resizeCount, state: state(instance) } };
}

reset();
{
  const input = [];
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push(data), () => {});
  emitInput("\x1b[?1;2c");
  const afterDa = { operations: [...operations], state: state(instance) };
  emitInput("\x1b[?7u");
  emitInput("\x1b[200~paste\ntext\x1b[201~");
  const afterKitty = { operations: [...operations], input, state: state(instance) };
  instance.stop();
  out.fallbackThenKitty = { afterDa, afterKitty, afterStop: { operations: [...operations], state: state(instance) } };
}

reset();
{
  const input = [];
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push(data), () => {});
  emitInput("\x1b[");
  advance(10);
  const afterFragmentFlush = { operations: [...operations], input: [...input], state: state(instance), now };
  emitInput("?7u");
  out.splitNegotiation = { afterFragmentFlush, afterCompletion: { operations: [...operations], input, state: state(instance), now } };
  instance.stop();
}

reset();
{
  const input = [];
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push({ data, at: now }), () => {});
  emitInput("\x1b[");
  advanceTo(1000);
  out.coarseTick = { input, target: now, state: state(instance) };
  instance.stop();
}

reset();
{
  const input = [];
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push(data), () => {});
  emitInput("\x1b[");
  advance(10);
  advance(149);
  const before = { input: [...input], now };
  advance(1);
  out.negotiationTimeout = { before, after: { input, now, state: state(instance) } };
  instance.stop();
}

reset();
{
  const input = [];
  const instance = new terminal.ProcessTerminal();
  instance.start((data) => input.push(data), () => {});
  emitInput("\x1b[?1;2c");
  const promise = instance.drainInput(120, 50);
  const afterBegin = { operations: [...operations], input: [...input], state: state(instance), now };
  for (let guard = 0; guard < 20; guard += 1) {
    const due = nextDue();
    if (!Number.isFinite(due)) break;
    advanceTo(due);
    await Promise.resolve();
  }
  await promise;
  out.drain = { afterBegin, afterDone: { operations: [...operations], input, state: state(instance), now } };
  instance.stop();
}

reset();
{
  const instance = new terminal.ProcessTerminal();
  instance.setProgress(true);
  instance.setProgress(true);
  advance(1000);
  const active = [...operations];
  instance.setProgress(false);
  instance.setProgress(false);
  instance.setProgress(true);
  instance.stop();
  out.progress = { active, complete: [...operations] };
}

reset({ width: 0, height: 0 });
{
  process.env.COLUMNS = "101";
  process.env.LINES = "37";
  const instance = new terminal.ProcessTerminal();
  const geometry = { columns: instance.columns, rows: instance.rows };
  instance.moveBy(2); instance.moveBy(-3); instance.moveBy(0);
  instance.hideCursor(); instance.showCursor(); instance.clearLine();
  instance.clearFromCursor(); instance.clearScreen(); instance.setTitle("hello");
  out.geometryAndWrites = { geometry, operations: [...operations] };
}

reset({ wasRaw: true });
{
  const instance = new terminal.ProcessTerminal();
  instance.start(() => {}, () => {});
  instance.stop();
  out.restoreRaw = { operations: [...operations], state: state(instance) };
}

reset();
{
  Object.defineProperty(process, "platform", { configurable: true, value: "win32" });
  const instance = new terminal.ProcessTerminal();
  instance.enableWindowsVTInput = () => operations.push("windows.enableVirtualTerminalInput");
  instance.start(() => {}, () => {});
  const afterStart = { operations: [...operations], state: state(instance) };
  instance.stop();
  out.windowsStart = { afterStart, afterStop: { operations: [...operations], state: state(instance) } };
  Object.defineProperty(process, "platform", saved.platform);
}

out.constants = {
  query: "\x1b[>7u\x1b[?u\x1b[c",
  bracketedPasteOn: "\x1b[?2004h",
  bracketedPasteOff: "\x1b[?2004l",
  kittyOff: "\x1b[<u",
  modifyOtherKeysOn: "\x1b[>4;2m",
  modifyOtherKeysOff: "\x1b[>4;0m",
  progressOn: "\x1b]9;4;3\x07",
  progressOff: "\x1b]9;4;0\x07",
};

// Restore host objects before writing or printing.
for (const [key, value] of Object.entries(saved.stdin)) process.stdin[key] = value;
for (const [key, value] of Object.entries(saved.stdout)) process.stdout[key] = value;
process.kill = saved.kill;
globalThis.setTimeout = saved.setTimeout;
globalThis.clearTimeout = saved.clearTimeout;
globalThis.setInterval = saved.setInterval;
globalThis.clearInterval = saved.clearInterval;
Date.now = saved.dateNow;
Object.defineProperty(process, "platform", saved.platform);
if (saved.isRaw) Object.defineProperty(process.stdin, "isRaw", saved.isRaw);
else delete process.stdin.isRaw;
if (saved.columns) Object.defineProperty(process.stdout, "columns", saved.columns);
else delete process.stdout.columns;
if (saved.rows) Object.defineProperty(process.stdout, "rows", saved.rows);
else delete process.stdout.rows;
if (saved.envColumns === undefined) delete process.env.COLUMNS; else process.env.COLUMNS = saved.envColumns;
if (saved.envLines === undefined) delete process.env.LINES; else process.env.LINES = saved.envLines;

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const fixturePath = join(root, "crates/pie-term/tests/fixtures/process-terminal.json");
writeFileSync(fixturePath, `${JSON.stringify(out, null, 2)}\n`);
console.log("wrote ProcessTerminal lifecycle fixture");
