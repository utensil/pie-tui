import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const repoRoot = join(packageRoot, '..', '..')
const api = JSON.parse(await readFile(join(repoRoot, 'tools', 'api-surface.json'), 'utf8'))
const declaration = await readFile(join(packageRoot, 'index.d.ts'), 'utf8')
const expected = api.statements.flatMap((statement) =>
  statement.symbols.map((symbol) => symbol.name),
)
const actual = new Set()

for (const match of declaration.matchAll(
  /export\s+(?:declare\s+)?(?:abstract\s+)?(?:class|interface|type|function|const)\s+([A-Za-z_$][\w$]*)/g,
)) {
  actual.add(match[1])
}
for (const match of declaration.matchAll(/export\s+(?:type\s+)?\{([^}]+)\}/g)) {
  for (const part of match[1].split(',')) {
    const name = part.trim().split(/\s+as\s+/).at(-1)
    if (name) actual.add(name)
  }
}

assert.deepEqual([...actual].sort(), [...expected].sort())
assert.equal(actual.size, 133)
console.log('type surface OK: exact 133-symbol canonical 0.84.2 namespace')
