import assert from "node:assert/strict";
import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const consumerRoot = process.env.CONSUMER_ROOT;
const consumerHead = process.env.CONSUMER_HEAD;
assert.ok(consumerRoot, "CONSUMER_ROOT is required");
assert.ok(consumerHead, "CONSUMER_HEAD is required");

const bridgeUrl = pathToFileURL(join(consumerRoot, "packages/tui/lib/bridge.js"));
const agentRoot = realpathSync(
  join(consumerRoot, "packages/tui/node_modules/@earendil-works/pi-coding-agent"),
);
const tuiRoot = join(dirname(agentRoot), "pi-tui");
const manifest = (
  await import(pathToFileURL(join(tuiRoot, "package.json")), {
    with: { type: "json" },
  })
).default;
const tui = await import(pathToFileURL(join(tuiRoot, "index.js")));
const agentModule = await import(pathToFileURL(join(agentRoot, "dist/index.js")));
const { createRuntimeHost } = await import(bridgeUrl);

assert.equal(manifest.name, "pie-tui-native", "packed facade override is active");
assert.equal(Object.keys(tui).length, 70, "runtime namespace remains exact");
assert.equal(typeof tui.setCapabilityOverrides, "function", "adopted overlay links");
assert.equal(typeof agentModule.InteractiveMode, "function", "InteractiveMode loaded through facade");

const scratch = mkdtempSync(join(tmpdir(), "pie-tui-dsh-live-"));
const listeners = new Map();
const sessionEvents = [];
const eventSubscribers = new Map();
const ctx = {
  on(name, cb) {
    eventSubscribers.set(name, cb);
    return () => eventSubscribers.delete(name);
  },
};
const agent = {
  session: {
    header: { cwd: scratch },
    events: sessionEvents,
    model: "deepseek-v4-flash",
    append(type, data) {
      const event = { type, data, seq: sessionEvents.length + 1 };
      sessionEvents.push(event);
      eventSubscribers.get("session/event")?.(agent.session, event);
      return { seq: event.seq };
    },
  },
  followup() {},
  steer() {},
  cancel() {},
  ctx: {
    on(name, cb) {
      const callbacks = listeners.get(name) ?? [];
      callbacks.push(cb);
      listeners.set(name, callbacks);
      return () => listeners.set(name, callbacks.filter((candidate) => candidate !== cb));
    },
  },
  options: {},
};

const originalWrite = process.stdout.write.bind(process.stdout);
const originalColumns = process.stdout.columns;
const originalRows = process.stdout.rows;
const writes = [];
const baselineInputListeners = process.stdin.listenerCount("data");
const baselineResizeListeners = process.stdout.listenerCount("resize");
let mode;
let runtimeHost;

process.env.PI_OFFLINE = "1";
process.env.PI_AGENT_DIR = scratch;
process.env.PI_CODING_AGENT_DIR = scratch;
Object.defineProperty(process.stdout, "columns", { configurable: true, value: 92 });
Object.defineProperty(process.stdout, "rows", { configurable: true, value: 28 });
process.stdout.write = (chunk, ..._args) => {
  writes.push(Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk));
  return true;
};

try {
  runtimeHost = createRuntimeHost(ctx, agent, "main-session-m6-live", {
    availableModels: [{ id: "deepseek-v4-flash", provider: "deepseek-official", name: "DeepSeek V4 Flash" }],
    defaultModel: "deepseek-v4-flash",
    fullscreenExitOutput: "none",
    hintSink() {},
    sessionsDir: join(scratch, "sessions"),
    theme: "dark",
    tuiMode: "fullscreen",
  });
  mode = new agentModule.InteractiveMode(runtimeHost, {
    initialThemeSetting: "dark",
    tuiMode: "fullscreen",
    verbose: false,
  });
  await mode.init();

  const emit = (type, data) => {
    const event = { type, data, seq: sessionEvents.length + 1 };
    sessionEvents.push(event);
    const subscriber = eventSubscribers.get("session/event");
    assert.equal(typeof subscriber, "function", "consumer bridge subscribed to dsh events");
    subscriber(agent.session, event);
  };
  emit("turn/start", { turn: 1 });
  emit("user/message", {
    content: [{ type: "text", text: "M6 live bridge input" }],
    source: { kind: "user" },
  });
  emit("assistant/message", {
    message: { content: [{ type: "text", text: "M6 live bridge receipt" }] },
  });
  emit("turn/end", { turn: 1, reason: { kind: "completed" } });
  await new Promise((resolve) => setTimeout(resolve, 40));
  mode.renderer.renderNow();

  Object.defineProperty(process.stdout, "columns", { configurable: true, value: 74 });
  Object.defineProperty(process.stdout, "rows", { configurable: true, value: 22 });
  process.stdout.emit("resize");
  process.stdin.emit("data", "m6-keypath");
  await new Promise((resolve) => setImmediate(resolve));

  mode.stop("none");
  runtimeHost.dispose();
  mode = undefined;
  runtimeHost = undefined;

  const output = writes.join("");
  assert.match(output, /\x1b\[\?1049h/, "fullscreen lifecycle entered alternate screen");
  assert.match(output, /\x1b\[\?1049l/, "fullscreen lifecycle exited alternate screen");
  assert.match(output, /M6 live bridge input/, "bridged user event rendered in the live pane");
  assert.match(output, /M6 live bridge receipt/, "bridged assistant event rendered in the live pane");
  assert.ok(writes.length >= 3, "initial, event, and resize/input renders reached the terminal");
  assert.equal(process.stdin.listenerCount("data"), baselineInputListeners, "terminal input listener cleaned up");
  assert.equal(process.stdout.listenerCount("resize"), baselineResizeListeners, "resize listener cleaned up");

  originalWrite(`${JSON.stringify({
    consumer: "dsh-pi-tui-mono",
    head: consumerHead,
    interactiveMode: "full-lifecycle",
    package: manifest.name,
    pieTuiExports: Object.keys(tui).length,
    renders: writes.length,
    terminal: "alternate-screen/input/resize/cleanup",
  })}\n`);
} finally {
  try {
    mode?.stop("none");
  } catch {}
  try {
    runtimeHost?.dispose();
  } catch {}
  tui.setCapabilityOverrides({});
  tui.resetCapabilitiesCache();
  process.stdout.write = originalWrite;
  Object.defineProperty(process.stdout, "columns", { configurable: true, value: originalColumns });
  Object.defineProperty(process.stdout, "rows", { configurable: true, value: originalRows });
  rmSync(scratch, { recursive: true, force: true });
}
