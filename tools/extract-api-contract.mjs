#!/usr/bin/env node
// Extract the canonical root-barrel API contract from pinned pi-tui declarations.
// PI_TUI_DIST=<pi-tui dist> node tools/extract-api-contract.mjs
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const dist = process.env.PI_TUI_DIST;
if (!dist) {
  console.error("PI_TUI_DIST not set");
  process.exit(64);
}
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const indexPath = join(dist, "index.d.ts");
const indexText = readFileSync(indexPath, "utf8");
const pkg = JSON.parse(readFileSync(join(dist, "..", "package.json"), "utf8"));
const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const normalize = (value) => value.replace(/\s+/g, " ").trim();

function findDeclaration(text, name) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(
    `export\\s+(?:declare\\s+)?(interface|type|class|function|const)\\s+${escaped}\\b`,
  ).exec(text);
  if (!match) return null;
  const kind = match[1];
  const start = match.index;
  let braces = 0;
  let brackets = 0;
  let parens = 0;
  let quote = null;
  let blockComment = false;
  let lineComment = false;
  let sawBrace = false;
  for (let i = start; i < text.length; i += 1) {
    const char = text[i];
    const next = text[i + 1];
    if (lineComment) {
      if (char === "\n") lineComment = false;
      continue;
    }
    if (blockComment) {
      if (char === "*" && next === "/") {
        blockComment = false;
        i += 1;
      }
      continue;
    }
    if (quote !== null) {
      if (char === "\\") {
        i += 1;
      } else if (char === quote) {
        quote = null;
      }
      continue;
    }
    if (char === "/" && next === "/") {
      lineComment = true;
      i += 1;
      continue;
    }
    if (char === "/" && next === "*") {
      blockComment = true;
      i += 1;
      continue;
    }
    if (char === '"' || char === "'" || char === "`") {
      quote = char;
      continue;
    }
    if (char === "{") {
      braces += 1;
      sawBrace = true;
    } else if (char === "}") {
      braces -= 1;
      if ((kind === "interface" || kind === "class") && sawBrace && braces === 0) {
        return { kind, signature: normalize(text.slice(start, i + 1)) };
      }
    } else if (char === "[") {
      brackets += 1;
    } else if (char === "]") {
      brackets -= 1;
    } else if (char === "(") {
      parens += 1;
    } else if (char === ")") {
      parens -= 1;
    } else if (char === ";" && braces === 0 && brackets === 0 && parens === 0) {
      return { kind, signature: normalize(text.slice(start, i + 1)) };
    }
  }
  throw new Error(`unterminated declaration for ${name}`);
}

function resolveDeclaration(filePath, name, visited = new Set()) {
  const visitKey = `${filePath}:${name}`;
  if (visited.has(visitKey)) throw new Error(`cyclic declaration re-export for ${name}`);
  visited.add(visitKey);

  const text = readFileSync(filePath, "utf8");
  const declaration = findDeclaration(text, name);
  if (declaration !== null) {
    return { ...declaration, declarationPath: filePath, declarationText: text };
  }

  for (const match of text.matchAll(/export\s+(?:type\s+)?\{([\s\S]*?)\}\s+from\s+"([^"]+)";/g)) {
    const specifier = match[2];
    if (!specifier.startsWith(".")) continue;
    for (const rawItem of match[1].split(",")) {
      const withoutType = rawItem.trim().replace(/^type\s+/, "");
      if (withoutType.length === 0) continue;
      const [imported, exported = imported] = withoutType.split(/\s+as\s+/);
      if (exported !== name) continue;
      const target = resolve(dirname(filePath), specifier.replace(/\.ts$/, ".d.ts"));
      return resolveDeclaration(target, imported, visited);
    }
  }
  throw new Error(`declaration not found for ${name} from ${filePath}`);
}

function sourcePath(specifier) {
  if (!specifier.startsWith(".")) return null;
  return join(dist, specifier.replace(/\.ts$/, ".d.ts"));
}

function sourceLabel(specifier) {
  return specifier.startsWith(".") ? specifier.replace(/^\.\//, "") : specifier;
}

function defaultMetadata(signature) {
  const optionalMarkerCount = [...signature.matchAll(/\b[A-Za-z_$][\w$]*\?\s*(?=[:(])/g)].length;
  const initializerCount = [...signature.matchAll(/\b[A-Za-z_$][\w$]*\s*=\s*[^,;)}]+/g)].length;
  const documented = [...signature.matchAll(/.{0,60}\bdefault\b.{0,80}/gi)]
    .map((match) => normalize(match[0]))
    .filter((value, index, values) => values.indexOf(value) === index);
  return { optionalMarkerCount, initializerCount, documented };
}

const statementMatches = [...indexText.matchAll(/export\s+(type\s+)?\{([\s\S]*?)\}\s+from\s+"([^"]+)";/g)];
const statements = statementMatches.map((match, index) => {
  const statementTypeOnly = Boolean(match[1]);
  const specifier = match[3];
  const stem = basename(specifier).replace(/\.(?:d\.)?ts$/, "").replace(/[^a-z0-9]+/gi, "-");
  const id = `S${String(index + 1).padStart(2, "0")}-${stem}`;
  const declarationPath = sourcePath(specifier);
  const declarationText = declarationPath === null ? null : readFileSync(declarationPath, "utf8");
  const names = match[2]
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean)
    .map((value) => {
      const itemTypeOnly = value.startsWith("type ");
      const withoutType = itemTypeOnly ? value.slice(5).trim() : value;
      const [imported, exported = imported] = withoutType.split(/\s+as\s+/);
      const exportKind = statementTypeOnly || itemTypeOnly ? "type" : "runtime";
      let declarationKind;
      let signature;
      let declarationSource;
      let declarationSourceSha256;
      if (declarationText === null) {
        declarationKind = exportKind === "type" ? "external-type" : "external-runtime";
        signature = `export ${exportKind} ${exported} from ${specifier}`;
        declarationSource = specifier;
        declarationSourceSha256 = null;
      } else {
        const resolvedDeclaration = resolveDeclaration(declarationPath, imported);
        ({ kind: declarationKind, signature } = resolvedDeclaration);
        declarationSource = relative(dist, resolvedDeclaration.declarationPath);
        declarationSourceSha256 = sha256(resolvedDeclaration.declarationText);
        const declarationExportKind = ["interface", "type"].includes(declarationKind)
          ? "type"
          : "runtime";
        if (declarationExportKind !== exportKind) {
          throw new Error(
            `${exported}: barrel says ${exportKind}, declaration ${declarationKind} says ${declarationExportKind}`,
          );
        }
      }
      return {
        name: exported,
        importedName: imported,
        exportKind,
        declarationKind,
        declarationSource,
        declarationSourceSha256,
        signature,
        signatureSha256: sha256(signature),
        defaultMetadata: defaultMetadata(signature),
      };
    });
  return {
    id,
    source: sourceLabel(specifier),
    sourceSha256: declarationText === null ? null : sha256(declarationText),
    barrelStatement: normalize(match[0]),
    barrelStatementSha256: sha256(normalize(match[0])),
    symbols: names,
  };
});

const symbols = statements.flatMap((statement) =>
  statement.symbols.map((symbol) => ({ statementId: statement.id, source: statement.source, ...symbol })),
);
if (statements.length !== 30 || symbols.length !== 133) {
  throw new Error(`unexpected canonical size: ${statements.length} statements / ${symbols.length} symbols`);
}
const duplicateNames = symbols
  .map((symbol) => symbol.name)
  .filter((name, index, names) => names.indexOf(name) !== index);
if (duplicateNames.length > 0) throw new Error(`duplicate root exports: ${duplicateNames.join(", ")}`);

const contract = {
  generator: "tools/extract-api-contract.mjs",
  reference: {
    package: pkg.name,
    version: pkg.version,
    indexSha256: sha256(indexText),
    statementCount: statements.length,
    symbolCount: symbols.length,
  },
  note: "Canonical root index.d.ts statements and constituent name/kind/signature/default metadata.",
  statements,
};
writeFileSync(join(root, "tools/api-surface.json"), `${JSON.stringify(contract, null, 2)}\n`);
console.log(`api contract written: ${statements.length} root statements / ${symbols.length} symbols`);
