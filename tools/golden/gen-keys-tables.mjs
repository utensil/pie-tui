import { readFileSync, writeFileSync } from "node:fs";
const src = readFileSync(process.env.PI_TUI_KEYS_JS, "utf8");
function grab(name) {
  const start = src.indexOf(`const ${name} =`);
  let i = src.indexOf("=", start) + 1, depth = 0, j = i, inStr = null;
  for (; j < src.length; j++) {
    const c = src[j];
    if (inStr) { if (c === "\\") { j++; continue; } if (c === inStr) inStr = null; continue; }
    if (c === '"' || c === "'" || c === "`") { inStr = c; continue; }
    if ("[{(".includes(c)) depth++;
    else if ("}])".includes(c)) { depth--; if (depth === 0) { j++; break; } }
  }
  return src.slice(i, j).trim();
}
const names=["SYMBOL_KEYS","MODIFIERS","LOCK_MASK","CODEPOINTS","ARROW_CODEPOINTS","FUNCTIONAL_CODEPOINTS","KITTY_FUNCTIONAL_KEY_EQUIVALENTS","LEGACY_KEY_SEQUENCES","LEGACY_SHIFT_SEQUENCES","LEGACY_CTRL_SEQUENCES","LEGACY_SEQUENCE_KEY_IDS"];
const vals={}; const scope={process:{env:{}}};
for (const k of Object.getOwnPropertyNames(globalThis.Symbol)) {}
// JSON-safe wrappers applied AFTER scope eval

for (const n of names) {
  const expr=grab(n);
  if (n==="LOCK_MASK"){ vals[n]=eval(`(()=>{${expr.replace(/^const |^let /,'').replace(';','')}})()`); continue; }
  // LOCK_MASK is `const X = 64 + 128;` style
}
// simpler: build one scope eval sequentially
for (const n of names) {
  const expr=grab(n);
  try { vals[n]=evalExprInScope(expr); scope[n]=vals[n]; }
  catch(e){ /* maybe plain expression like '64 + 128' */ vals[n]=eval(expr); scope[n]=vals[n]; }
}
function evalExprInScope(expr){
  const keys=Object.keys(scope);
  return Function(...keys, `"use strict"; return (${expr});`)(...keys.map(k=>scope[k]));
}
vals.SYMBOL_KEYS=[...vals.SYMBOL_KEYS];
vals.KITTY_FUNCTIONAL_KEY_EQUIVALENTS=[...vals.KITTY_FUNCTIONAL_KEY_EQUIVALENTS];
writeFileSync("/tmp/keys-tables.json", JSON.stringify(vals,null,1));
console.log("extracted:",names.join(","));
