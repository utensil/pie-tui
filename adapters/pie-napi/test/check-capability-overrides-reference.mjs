import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, posix } from 'node:path'
import { pathToFileURL } from 'node:url'

import * as local from '../index.js'
import fixture from './oracle-contract.json' with { type: 'json' }
import drift from './upstream-drift.json' with { type: 'json' }

const tarball = process.env.PI_TUI_0844_TARBALL
assert.ok(tarball, 'PI_TUI_0844_TARBALL is required')

const expected = drift.observedLatest
const bytes = await readFile(tarball)
assert.equal(
  createHash('sha256').update(bytes).digest('hex'),
  expected.tarballSha256,
  '0.84.4 tarball SHA-256',
)
assert.equal(
  `sha512-${createHash('sha512').update(bytes).digest('base64')}`,
  expected.tarballIntegrity,
  '0.84.4 tarball npm integrity',
)

const entries = execFileSync('tar', ['-tzf', tarball], { encoding: 'utf8' })
  .split('\n')
  .filter(Boolean)
for (const entry of entries) {
  assert.equal(posix.isAbsolute(entry), false, `absolute tar entry: ${entry}`)
  assert.equal(posix.normalize(entry), entry, `unsafe tar entry: ${entry}`)
}

const extractionRoot = await mkdtemp(join(tmpdir(), 'pie-tui-overlay-oracle-'))
const environmentKeys = [
  ...fixture.capabilityEnvironmentKeys,
  'PI_HYPERLINKS',
  'PI_IMAGE_PROTOCOL',
  'PI_TRUE_COLOR',
]

function exercise(api) {
  const previous = new Map(environmentKeys.map((key) => [key, process.env[key]]))
  try {
    for (const key of environmentKeys) delete process.env[key]
    process.env.COLORTERM = 'truecolor'

    api.setCapabilityOverrides({})
    api.resetCapabilitiesCache()
    const baseline = api.getCapabilities()
    const baselineIdentityStable = api.getCapabilities() === baseline

    const input = { images: 'kitty' }
    api.setCapabilityOverrides(input)
    input.images = null
    const first = api.getCapabilities()
    const clonedInput = first !== input && first.images === 'kitty'

    api.setCapabilityOverrides({ images: 'kitty' })
    const equalPreservedIdentity = api.getCapabilities() === first

    api.setCapabilityOverrides({ images: 'kitty', trueColor: false })
    const changed = api.getCapabilities()
    const changedInvalidatedIdentity = changed !== first

    const temporary = { images: 'iterm2', trueColor: true, hyperlinks: true }
    api.setCapabilities(temporary)
    const temporaryIdentity = api.getCapabilities() === temporary
    api.resetCapabilitiesCache()
    const resumed = api.getCapabilities()

    api.setCapabilityOverrides({})
    const restored = api.getCapabilities()

    let nullError
    try {
      api.setCapabilityOverrides(null)
    } catch (error) {
      nullError = error?.name
    }

    return {
      baseline,
      baselineIdentityStable,
      first,
      clonedInput,
      equalPreservedIdentity,
      changed,
      changedInvalidatedIdentity,
      temporaryIdentity,
      resumed,
      restored,
      nullError,
    }
  } finally {
    api.setCapabilityOverrides({})
    api.resetCapabilitiesCache()
    for (const key of environmentKeys) delete process.env[key]
    for (const [key, value] of previous) {
      if (value !== undefined) process.env[key] = value
    }
  }
}

try {
  execFileSync('tar', ['-xzf', tarball, '-C', extractionRoot], { stdio: 'pipe' })
  const packageRoot = join(extractionRoot, 'package')
  const manifest = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8'))
  assert.equal(manifest.name, '@earendil-works/pi-tui')
  assert.equal(manifest.version, expected.version)

  const terminalImageJs = await readFile(
    join(packageRoot, 'dist', 'terminal-image.js'),
  )
  const terminalImageDts = await readFile(
    join(packageRoot, 'dist', 'terminal-image.d.ts'),
  )
  assert.equal(
    createHash('sha256').update(terminalImageJs).digest('hex'),
    expected.terminalImageJsSha256,
    'terminal-image.js SHA-256',
  )
  assert.equal(
    createHash('sha256').update(terminalImageDts).digest('hex'),
    expected.terminalImageDtsSha256,
    'terminal-image.d.ts SHA-256',
  )
  assert.match(
    terminalImageDts.toString('utf8'),
    new RegExp(
      fixture.adoptedRuntimeOverlays.setCapabilityOverrides.declaration
        .replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
    ),
  )

  const reference = await import(
    pathToFileURL(join(packageRoot, 'dist', 'terminal-image.js'))
  )
  assert.deepEqual(exercise(local), exercise(reference))
  console.log(
    'capability overlay oracle OK: authenticated 0.84.4 reference matches the adopted overlay',
  )
} finally {
  await rm(extractionRoot, { recursive: true, force: true })
}
