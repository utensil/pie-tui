// gen-golden-components.mjs — drive the pinned pi-tui components through
// scripted render sequences; record outputs for the Rust golden test.
//   PI_TUI_DIST=... node tools/golden/gen-golden-components.mjs
import { writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
const load = async (file, symbol) => (await import("file://" + join(dist, "components", file)))[symbol];
const [Text, TruncatedText, Spacer, Box, VStack, HStack, Loader] = await Promise.all([
  load("text.js", "Text"),
  load("truncated-text.js", "TruncatedText"),
  load("spacer.js", "Spacer"),
  load("box.js", "Box"),
  load("v-stack.js", "VStack"),
  load("h-stack.js", "HStack"),
  load("loader.js", "Loader"),
]);

const cyan = (s) => `\x1b[36m${s}\x1b[0m`;
const dim = (s) => `\x1b[2m${s}\x1b[0m`;
const bgGreen = (s) => `\x1b[42m${s}\x1b[0m`;

// Each case: { name, build() -> component, script(component) -> outputs[] }
// The Rust test replicates the same script per name.
const cases = [
  {
    name: "text-default-plain",
    build: () => new Text("hello world\nsecond line"),
    script: (c) => [c.render(20), c.render(40)],
  },
  {
    name: "text-ansi-cjk",
    build: () => new Text("mix \x1b[4munder\u6f22\u5b57\x1b[24m tail\n\ttabbed"),
    script: (c) => [c.render(12), c.render(30)],
  },
  {
    name: "text-bgfn",
    build: () => new Text("bg text here", 1, 1, bgGreen),
    script: (c) => [c.render(16)],
  },
  {
    name: "text-settext-cache",
    build: () => new Text("before", 0, 0),
    script: (c) => {
      const out = [c.render(20)];
      c.setText("after with longer content wrapping");
      out.push(c.render(20));
      return out;
    },
  },
  {
    name: "text-empty-whitespace",
    build: () => new Text("  \n\t "),
    script: (c) => [c.render(10)],
  },
  {
    name: "truncated-default",
    build: () => new TruncatedText("a very long line that will definitely need truncation"),
    script: (c) => [c.render(20), c.render(80)],
  },
  {
    name: "truncated-ansi-newline-cjk",
    build: () => new TruncatedText("\x1b[31mred\u6f22\u5b57 styled\x1b[0m tail\nhidden second", 2, 1),
    script: (c) => [c.render(18)],
  },
  {
    name: "spacer-1-3",
    build: () => new Spacer(1),
    script: (c) => [c.render(10), (c.setLines(3), c.render(10))],
  },
  {
    name: "box-children",
    build: () => {
      const b = new Box(1, 1);
      b.addChild(new Text("one", 0, 0));
      b.addChild(new Text("two", 0, 0));
      return b;
    },
    script: (c) => [c.render(14)],
  },
  {
    name: "box-bgfn",
    build: () => {
      const b = new Box(2, 1, bgGreen);
      b.addChild(new Text("boxed", 0, 0));
      return b;
    },
    script: (c) => [c.render(16)],
  },
  {
    name: "box-empty",
    build: () => new Box(1, 1),
    script: (c) => [c.render(10)],
  },
  {
    name: "vstack-gap0",
    build: () => new VStack([new Text("top", 0, 0), new Spacer(1), new Text("bottom", 0, 0)]),
    script: (c) => [c.render(12)],
  },
  {
    name: "vstack-gap2-grow",
    build: () => {
      const v = new VStack([], { gap: 2 });
      v.addChild(new Text("fixed", 0, 0), { grow: 0 });
      v.addChild(new Text("grows", 0, 0), { grow: 1 });
      return v;
    },
    script: (c) => [c.render(10)],
  },
  {
    name: "hstack-align-default",
    build: () => new HStack([new Text("L1\nL2\nL3", 0, 0), new Text("R1\nR2", 0, 0)], { gap: 2 }),
    script: (c) => [c.render(20)],
  },
  {
    name: "hstack-align-center-end",
    build: () => new HStack([new Text("a\nbb\nccc", 0, 0), new Text("X\nY", 0, 0)], { gap: 1, align: "center" }),
    script: (c) => [c.render(14)],
  },
  {
    name: "loader-frames",
    build: () => new Loader(null, cyan, dim, "Loading stuff..."),
    script: (c) => {
      const out = [c.render(24)];
      c.currentFrame = 1;
      c.updateDisplay();
      out.push(c.render(24));
      c.currentFrame = 5;
      c.updateDisplay();
      out.push(c.render(24));
      return out;
    },
  },
];

const results = cases.map(({ name, build, script }) => {
  const c = build();
  try {
    const outputs = script(c);
    return { name, outputs };
  } finally {
    if (c instanceof Loader) c.stop();
  }
});

const outDir = resolve(dirname(fileURLToPath(import.meta.url)), "../../crates/pie-components/tests");
writeFileSync(
  join(outDir, "fixtures/components-golden.json"),
  JSON.stringify({ generator: "gen-golden-components.mjs", reference: process.env.PI_TUI_REF_VERSION ?? "0.84.1", cases: results }, null, 1) + "\n"
);
console.log(`harvested ${results.length} component cases`);
