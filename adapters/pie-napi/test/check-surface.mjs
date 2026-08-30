#!/usr/bin/env node
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import fixture from './oracle-contract.json' with { type: 'json' }

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..')
const runtimeSource = readFileSync(join(packageRoot, 'runtime.cjs'), 'utf8')
const esmSource = readFileSync(join(packageRoot, 'index.js'), 'utf8')
const cjsSource = readFileSync(join(packageRoot, 'index.cjs'), 'utf8')
const expectedNames = Object.keys(fixture.runtimeTypes)

const selectedBlock = runtimeSource.match(
  /const selectedExports = \{\n(?<body>[\s\S]*?)\n\}\n\nconst runtimeTarget = Object\.create\(null\)/,
)
assert.ok(selectedBlock?.groups?.body, 'runtime.cjs selectedExports block is missing')

const selectedLines = selectedBlock.groups.body.split('\n')
assert.ok(selectedLines.length > 0, 'runtime.cjs selectedExports block is empty')
const actualNames = selectedLines.map((line) => {
  const match = line.match(/^  (?<name>[A-Za-z_$][\w$]*),$/)
  assert.ok(match?.groups?.name, `non-shorthand selected export: ${JSON.stringify(line)}`)
  return match.groups.name
})

assert.deepEqual(
  actualNames,
  expectedNames,
  'runtime.cjs selected export order/count differs from the pinned package oracle',
)
assert.equal(new Set(actualNames).size, actualNames.length, 'duplicate selected runtime export')

const esmBlock = esmSource.match(/export const \{\n(?<body>[\s\S]*?)\n\} = runtime\n?$/)
assert.ok(esmBlock?.groups?.body, 'index.js runtime export block is missing')
const esmNames = esmBlock.groups.body.split('\n').map((line) => {
  const match = line.match(/^  (?<name>[A-Za-z_$][\w$]*),$/)
  assert.ok(match?.groups?.name, `non-shorthand ESM export: ${JSON.stringify(line)}`)
  return match.groups.name
})
assert.equal(new Set(esmNames).size, esmNames.length, 'duplicate ESM runtime export')
assert.deepEqual(
  [...esmNames].sort(),
  [...expectedNames].sort(),
  'index.js export membership differs from the pinned package oracle',
)
assert.equal(
  cjsSource,
  "'use strict'\n\nmodule.exports = require('./runtime.cjs')\n",
  'index.cjs must re-export the exact shared runtime object',
)

console.log(`surface OK: ${actualNames.length} ordered shared runtime exports`)
