import assert from 'node:assert/strict'
import { exerciseNaturalCopyControls } from './natural-selection-scenario.mjs'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

const distribution = process.env.PI_TUI_0844_DIST
assert.ok(distribution, 'PI_TUI_0844_DIST must point to authenticated pi-tui 0.84.4 dist')
const manifest = JSON.parse(await readFile(join(dirname(distribution), 'package.json'), 'utf8'))
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.4')
const sourceHashes = {
  'tui-alt-screen.js': '4e82f4d560558e4d704ddbf2414c3def2a14c28aaaa2985ff571dedec074946d',
  'tui-alt-screen.d.ts': '87165a2ced929067e379ccca8d226a6e07859012a8fb65c49b738594aa4e0729',
}
for (const [name, expected] of Object.entries(sourceHashes)) {
  const actual = createHash('sha256').update(await readFile(join(distribution, name))).digest('hex')
  assert.equal(actual, expected, `${name} SHA-256`)
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')

assert.deepEqual(
  await exerciseNaturalCopyControls(adapter),
  await exerciseNaturalCopyControls(reference),
)
console.log('copy-control oracle passed: natural root/ScrollView selection, callback, OSC 52, and clear')
