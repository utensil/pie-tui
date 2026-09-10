import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'
import {
  assertPublicNegativeWidthGuards, narrowDialogScenarios,
} from './narrow-dialogs-scenarios.mjs'

const distribution = process.env.PI_TUI_DIST
assert.ok(distribution, 'PI_TUI_DIST must point to authenticated pi-tui 0.84.2 dist')
const packageRoot = dirname(distribution)
const manifest = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8'))
assert.equal(manifest.name, '@earendil-works/pi-tui')
assert.equal(manifest.version, '0.84.2')
for (const [file, digest] of Object.entries({
  'package.json': '2c19fb7e3d1e83a461b6f020b2ffc118b435dcd78a07af8c8def72864cd09e6e',
  'dist/components/select-list.js': 'ea14ebd2f64ed045563360b598eeccc816f7f9f252df6b7bc492309cfe49c545',
  'dist/components/settings-list.js': '475f324eb9b077d3f2b90aed72f9972cc1fa6c53421d517514180812f95343f2',
})) {
  assert.equal(
    createHash('sha256').update(await readFile(join(packageRoot, file))).digest('hex'),
    digest, `authenticated narrow dialogs ${file} SHA-256`,
  )
}

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
const adapter = await import('../index.js')
const expected = narrowDialogScenarios(reference)
const actual = narrowDialogScenarios(adapter)
assert.equal(expected.length, 35)
for (const row of expected) {
  assert.equal(row.error, undefined, `reference ${row.kind} width ${row.width}`)
}
assert.deepEqual(actual, expected, 'narrow dialogs match reference render bytes, input and callback arguments')
assertPublicNegativeWidthGuards(adapter)
console.log('Narrow dialogs authenticated oracle OK: 35 paired cases and public width guards')
