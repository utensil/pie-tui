import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

const consumerRoot = process.env.CONSUMER_ROOT;
const consumerHead = process.env.CONSUMER_HEAD;
const receiptPath = process.env.RECEIPT_PATH;
const streamAckPath = process.env.STREAM_ACK_PATH;
const expectedTuiPackage = process.env.EXPECTED_TUI_PACKAGE ?? "pie-tui-native";
assert.ok(consumerRoot, "CONSUMER_ROOT is required");
assert.ok(consumerHead, "CONSUMER_HEAD is required");
assert.ok(receiptPath, "RECEIPT_PATH is required");
assert.ok(streamAckPath, "STREAM_ACK_PATH is required");
assert.equal(process.stdin.isTTY, true, "consumer stdin must be a real TTY");
assert.equal(process.stdout.isTTY, true, "consumer stdout must be a real TTY");

const bridgeUrl = pathToFileURL(join(consumerRoot, "packages/tui/lib/bridge.js"));
const agentRoot = realpathSync(
  join(consumerRoot, "packages/tui/node_modules/@earendil-works/pi-coding-agent"),
);
const tuiRoot = join(dirname(agentRoot), "pi-tui");
const tuiManifest = JSON.parse(readFileSync(join(tuiRoot, "package.json"), "utf8"));
assert.equal(tuiManifest.name, expectedTuiPackage, "expected packed facade override is active");
const agentUrl = pathToFileURL(join(agentRoot, "dist/index.js"));
const { InteractiveMode } = await import(agentUrl);
const { createRuntimeHost } = await import(bridgeUrl);

const baselineInputListeners = process.stdin.listenerCount("data");
const baselineResizeListeners = process.stdout.listenerCount("resize");
const baselineStty = execFileSync("stty", ["-g"], {
  encoding: "utf8",
  stdio: [0, "pipe", "pipe"],
}).trim();
const result = {
  backend: "deterministic-fake",
  consumer: "dsh-pi-tui-mono",
  head: consumerHead,
  package: tuiManifest.name,
  realTty: true,
  ready: false,
  streamAcknowledged: false,
  resizeWidths: [],
  settingsChanges: [],
  submittedText: null,
  terminalRestored: false,
  listenersRestored: false,
};

mkdirSync(dirname(receiptPath), { recursive: true });
const save = () => writeFileSync(receiptPath, `${JSON.stringify(result, null, 2)}\n`);
const observeResize = () => { result.resizeWidths.push(process.stdout.columns); result.lastResizeWidth = process.stdout.columns; save(); };
process.stdout.on("resize", observeResize);
process.on("exit", (code) => {
  process.stdout.off("resize", observeResize);
  result.exitCode = code;
  try {
    result.terminalRestored = execFileSync("stty", ["-g"], {
      encoding: "utf8",
      stdio: [0, "pipe", "pipe"],
    }).trim() === baselineStty;
  } catch (error) {
    result.terminalError = error?.message ?? String(error);
  }
  result.listenersRestored =
    process.stdin.listenerCount("data") === baselineInputListeners &&
    process.stdout.listenerCount("resize") === baselineResizeListeners;
  save();
});

const scratch = dirname(receiptPath);
const eventSubscribers = new Map();
const sessionEvents = [];
const ctx = {
  on(name, callback) {
    eventSubscribers.set(name, callback);
    return () => eventSubscribers.delete(name);
  },
};

let agent;
const append = (type, data) => {
  const event = { type, data, seq: sessionEvents.length + 1 };
  sessionEvents.push(event);
  eventSubscribers.get("session/event")?.(agent.session, event);
};
const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));
const waitForStreamAck = async () => {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    if (existsSync(streamAckPath)) {
      result.streamAcknowledged = true;
      save();
      return;
    }
    await delay(25);
  }
  throw new Error("timed out waiting for the tmux gate to observe streaming output");
};
const runFixtureTurn = async (message) => {
  result.submittedText = message?.content?.find((block) => block.type === "text")?.text ?? "";
  save();
  append("turn/start", { turn: 1 });
  append("step/start", { turn: 1, step: 1 });
  append("user/message", { content: message.content, source: { kind: "user" } });
  await delay(120);
  append("assistant/chunk", { chunk: { type: "block-start", blockType: "text" } });
  append("assistant/chunk", { chunk: { type: "text-delta", text: "stream-only-marker" } });
  await waitForStreamAck();
  await delay(120);
  append("assistant/message", {
    message: {
      content: [
        { type: "text", text: "deterministic final answer" },
        { type: "tool-call", id: "fixture-call", name: "fixture_tool", arguments: '{"value":"fixture-input"}' },
      ],
    },
  });
  append("tool/call", {
    callId: "fixture-call",
    name: "fixture_tool",
    arguments: '{"value":"fixture-input"}',
  });
  await delay(120);
  append("tool/result", {
    message: {
      source: { callId: "fixture-call" },
      content: [{
        type: "tool-result",
        toolCallId: "fixture-call",
        content: [{ type: "text", text: "fixture-tool-output" }],
        isError: false,
      }],
    },
  });
  append("step/end", { turn: 1, step: 1 });
  append("step/start", { turn: 1, step: 2 });
  append("assistant/chunk", { chunk: { type: "block-start", blockType: "text" } });
  append("assistant/chunk", { chunk: { type: "text-delta", text: "fixture backend complete" } });
  await delay(120);
  append("assistant/message", {
    message: { content: [{ type: "text", text: "fixture backend complete" }] },
  });
  append("step/end", { turn: 1, step: 2 });
  append("turn/end", { turn: 1, reason: { kind: "completed" } });
};

agent = {
  session: {
    header: { cwd: scratch },
    events: sessionEvents,
    model: "fixture-model",
    append,
  },
  followup(message) {
    void runFixtureTurn(message).catch((error) => {
      result.backendError = error?.stack ?? String(error);
      save();
      process.exitCode = 1;
    });
  },
  steer() {},
  cancel() {},
  ctx: { on: () => () => {} },
  options: {},
};

process.env.PI_OFFLINE = "1";
process.env.PI_AGENT_DIR = scratch;
process.env.PI_CODING_AGENT_DIR = scratch;
const runtimeHost = createRuntimeHost(ctx, agent, "consumer-harness", {
  availableModels: [{ id: "fixture-model", provider: "fixture", name: "Fixture Model" }],
  defaultModel: "fixture-model",
  fullscreenExitOutput: "none",
  hintSink() {},
  sessionsDir: join(scratch, "sessions"),
  theme: "dark",
  tuiMode: "fullscreen",
});
const originalSetAutoCompaction = runtimeHost.session.setAutoCompactionEnabled.bind(runtimeHost.session);
runtimeHost.session.setAutoCompactionEnabled = (enabled) => {
  result.settingsChanges.push({ enabled, columns: process.stdout.columns });
  save();
  return originalSetAutoCompaction(enabled);
};
const mode = new InteractiveMode(runtimeHost, {
  initialThemeSetting: "dark",
  tuiMode: "fullscreen",
  verbose: false,
});
await mode.init();
result.ready = true;
save();
await mode.run();
