import assert from "node:assert/strict";
import test from "node:test";

const identity = (text) => text;
const editorTheme = {
  borderColor: identity,
  selectList: {
    selectedPrefix: identity,
    selectedText: identity,
    description: identity,
    scrollInfo: identity,
    noMatch: identity,
  },
};

function makeEditor(api) {
  const renders = [];
  const tui = {
    terminal: { rows: 24 },
    requestRender: (force) => renders.push(force),
  };
  return { editor: new api.Editor(tui, editorTheme), renders };
}

function replacePrefix(lines, cursorLine, cursorCol, prefix, replacement) {
  const next = [...lines];
  const line = next[cursorLine];
  const start = cursorCol - prefix.length;
  next[cursorLine] = `${line.slice(0, start)}${replacement}${line.slice(cursorCol)}`;
  return {
    lines: next,
    cursorLine,
    cursorCol: start + replacement.length,
  };
}

async function waitFor(predicate, message) {
  const deadline = Date.now() + 1_000;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  assert.fail(message);
}

function type(editor, text) {
  for (const character of text) editor.handleInput(character);
}

function commandProvider(trace) {
  return {
    async getSuggestions(lines, cursorLine, cursorCol, options) {
      trace.getReceivers.push(this);
      trace.requests.push({ lines: [...lines], cursorLine, cursorCol, force: options.force });
      return {
        prefix: lines[cursorLine].slice(0, cursorCol),
        items: [
          { value: "model", label: "model", description: "Select model" },
          { value: "more", label: "more", description: "Show more" },
        ],
      };
    },
    applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
      trace.applyReceivers.push(this);
      trace.applied.push(item.value);
      return replacePrefix(lines, cursorLine, cursorCol, prefix, `/${item.value} `);
    },
  };
}

export function registerAutocompleteTests(api, label) {
  test(`${label}: slash menu supports selection, Tab, Enter, and cancel`, async () => {
    const trace = { requests: [], applied: [], getReceivers: [], applyReceivers: [] };
    const provider = commandProvider(trace);
    const { editor } = makeEditor(api);
    editor.setAutocompleteProvider(provider);

    type(editor, "/mo");
    await waitFor(() => editor.isShowingAutocomplete(), "slash menu did not open");
    const menu = editor.render(50).join("\n");
    assert.match(menu, /model/);
    assert.match(menu, /more/);
    assert.deepEqual(trace.requests.at(-1), {
      lines: ["/mo"], cursorLine: 0, cursorCol: 3, force: false,
    });

    editor.handleInput("\x1b[B");
    editor.handleInput("\t");
    assert.equal(editor.getText(), "/more ");
    assert.equal(editor.isShowingAutocomplete(), false);

    editor.setText("");
    const submitted = [];
    editor.onSubmit = (value) => submitted.push(value);
    type(editor, "/mo");
    await waitFor(() => editor.isShowingAutocomplete(), "slash menu did not reopen");
    editor.handleInput("\r");
    assert.deepEqual(submitted, ["/model"]);

    type(editor, "/mo");
    await waitFor(() => editor.isShowingAutocomplete(), "slash menu did not open for cancel");
    editor.handleInput("\x1b");
    assert.equal(editor.isShowingAutocomplete(), false);
    assert.equal(editor.getText(), "/mo");
    assert.ok(trace.getReceivers.every((receiver) => receiver === provider));
    assert.ok(trace.applyReceivers.every((receiver) => receiver === provider));
  });

  test(`${label}: forced file completion preserves UTF-16 cursor units`, async () => {
    const calls = [];
    const provider = {
      shouldTriggerFileCompletion(lines, cursorLine, cursorCol) {
        calls.push({ kind: "should", receiver: this, lines: [...lines], cursorLine, cursorCol });
        return true;
      },
      async getSuggestions(lines, cursorLine, cursorCol, options) {
        calls.push({ kind: "get", receiver: this, lines: [...lines], cursorLine, cursorCol, force: options.force });
        return { prefix: "fi", items: [{ value: "file.txt", label: "file.txt" }] };
      },
      applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
        calls.push({ kind: "apply", receiver: this, cursorLine, cursorCol, value: item.value, prefix });
        return replacePrefix(lines, cursorLine, cursorCol, prefix, item.value);
      },
    };
    const { editor, renders } = makeEditor(api);
    const changes = [];
    editor.onChange = (text) => changes.push(text);
    editor.setAutocompleteProvider(provider);
    editor.setText("say 😀 fi");
    assert.equal(editor.getCursor().col, "say 😀 fi".length);
    const rendersBeforeTab = renders.length;
    editor.handleInput("\t");
    await waitFor(() => editor.getText().endsWith("file.txt"), "forced completion did not apply");

    assert.equal(calls[0].kind, "should");
    assert.equal(calls[1].kind, "get");
    assert.equal(calls[1].cursorCol, 9, "provider cursor uses UTF-16 code units");
    assert.equal(calls[1].force, true);
    assert.equal(calls[2].kind, "apply");
    assert.ok(calls.every((call) => call.receiver === provider));
    assert.equal(editor.getText(), "say 😀 file.txt");
    assert.deepEqual(editor.getCursor(), { line: 0, col: 15 });
    assert.equal(changes.at(-1), "say 😀 file.txt");
    assert.ok(renders.length > rendersBeforeTab, "completion requests a render");
  });

  test(`${label}: a custom trigger character activates at a token boundary`, async () => {
    const requests = [];
    const provider = {
      triggerCharacters: ["!"],
      async getSuggestions(lines, cursorLine, cursorCol, options) {
        requests.push({ lines: [...lines], cursorLine, cursorCol, force: options.force });
        return { prefix: "!x", items: [{ value: "extra", label: "extra" }] };
      },
      applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
        return replacePrefix(lines, cursorLine, cursorCol, prefix, item.value);
      },
    };
    const { editor } = makeEditor(api);
    editor.setAutocompleteProvider(provider);
    type(editor, "say !x");
    await waitFor(() => editor.isShowingAutocomplete(), "custom trigger did not open menu");
    assert.deepEqual(requests.at(-1), {
      lines: ["say !x"], cursorLine: 0, cursorCol: 6, force: false,
    });
    assert.match(editor.render(40).join("\n"), /extra/);
  });

  test(`${label}: replacement aborts stale async work and ignores its result`, async () => {
    let resolveSlow;
    let slowSignal;
    let slowCalls = 0;
    const slow = {
      async getSuggestions(_lines, _line, _col, options) {
        slowCalls += 1;
        slowSignal = options.signal;
        return new Promise((resolve) => { resolveSlow = resolve; });
      },
      applyCompletion() { assert.fail("stale provider completion applied"); },
    };
    let fastCalls = 0;
    const fast = {
      async getSuggestions(lines, line, col) {
        fastCalls += 1;
        return { prefix: lines[line].slice(0, col), items: [{ value: "fresh", label: "fresh" }] };
      },
      applyCompletion(lines, line, col, item, prefix) {
        return replacePrefix(lines, line, col, prefix, `/${item.value} `);
      },
    };
    const { editor } = makeEditor(api);
    editor.setAutocompleteProvider(slow);
    editor.handleInput("/");
    await waitFor(() => slowCalls === 1, "slow request did not start");
    editor.setAutocompleteProvider(fast);
    assert.equal(slowSignal.aborted, true);
    editor.handleInput("f");
    resolveSlow({ prefix: "/", items: [{ value: "stale", label: "stale" }] });
    await waitFor(() => fastCalls === 1, "replacement provider did not run");
    await waitFor(() => editor.isShowingAutocomplete(), "replacement result did not open menu");
    const rendered = editor.render(40).join("\n");
    assert.match(rendered, /fresh/);
    assert.doesNotMatch(rendered, /stale/);
  });

  test(`${label}: reentrant and throwing callbacks can be replaced and recovered`, async () => {
    const { editor } = makeEditor(api);
    let recoveryCalls = 0;
    const recovery = {
      shouldTriggerFileCompletion() { return true; },
      async getSuggestions(lines, line, col) {
        recoveryCalls += 1;
        return { prefix: lines[line].slice(0, col), items: [{ value: "recovered", label: "recovered" }] };
      },
      applyCompletion(lines, line, col, item, prefix) {
        return replacePrefix(lines, line, col, prefix, item.value);
      },
    };
    const reentrant = {
      async getSuggestions() {
        editor.setAutocompleteProvider(recovery);
        return { prefix: "/", items: [{ value: "stale", label: "stale" }] };
      },
      applyCompletion() { assert.fail("reentrant stale result applied"); },
    };
    editor.setAutocompleteProvider(reentrant);
    editor.handleInput("/");
    await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(editor.isShowingAutocomplete(), false);
    editor.setText("fi");
    editor.handleInput("\t");
    await waitFor(() => editor.getText() === "recovered", "reentrant replacement did not recover");
    assert.equal(recoveryCalls, 1);

    const throwing = {
      shouldTriggerFileCompletion() { throw new Error("fixture trigger failure"); },
      async getSuggestions() { assert.fail("throwing provider should not request suggestions"); },
      applyCompletion() { assert.fail("throwing provider should not apply"); },
    };
    editor.setText("again");
    editor.setAutocompleteProvider(throwing);
    assert.throws(() => editor.handleInput("\t"), /fixture trigger failure/);
    editor.setAutocompleteProvider(recovery);
    editor.handleInput("\t");
    await waitFor(() => editor.getText() === "recovered", "throwing provider replacement did not recover");
    assert.equal(recoveryCalls, 2);
  });
  test(`${label}: text changes precede provider requests and reentrant edits`, async () => {
    const { editor } = makeEditor(api);
    const trace = [];
    editor.onChange = (text) => trace.push(["change", text]);
    editor.setAutocompleteProvider({
      async getSuggestions() {
        trace.push(["request", editor.getText()]);
        editor.setText("replacement");
        return null;
      },
      applyCompletion() { assert.fail("empty suggestions must not apply"); },
    });
    editor.handleInput("/");
    await waitFor(() => trace.some(([kind]) => kind === "request"), "provider request did not start");
    assert.deepEqual(trace, [
      ["change", "/"],
      ["request", "/"],
      ["change", "replacement"],
    ]);
    assert.equal(editor.getText(), "replacement");
  });

}
