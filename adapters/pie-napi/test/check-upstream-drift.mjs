import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, posix } from 'node:path'

import oracle from './oracle-contract.json' with { type: 'json' }
import drift from './upstream-drift.json' with { type: 'json' }

const inputs = [
  [process.env.PI_TUI_0842_TARBALL, drift.baseline],
  [process.env.PI_TUI_0844_TARBALL, drift.observedLatest],
]
for (const [path, expected] of inputs) {
  assert.ok(path, `tarball path is required for ${expected.version}`)
}

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const sha512 = (bytes) =>
  `sha512-${createHash('sha512').update(bytes).digest('base64')}`
const extractionRoot = await mkdtemp(join(tmpdir(), 'pie-tui-drift-'))

try {
  const exportsByVersion = new Map()
  for (const [tarball, expected] of inputs) {
    const bytes = await readFile(tarball)
    assert.equal(sha256(bytes), expected.tarballSha256, `${expected.version} tar SHA-256`)
    assert.equal(sha512(bytes), expected.tarballIntegrity, `${expected.version} npm integrity`)
    const entries = execFileSync('tar', ['-tzf', tarball], { encoding: 'utf8' })
      .split('\n')
      .filter(Boolean)
    for (const entry of entries) {
      assert.equal(posix.isAbsolute(entry), false, `absolute tar entry: ${entry}`)
      assert.equal(posix.normalize(entry), entry, `unsafe tar entry: ${entry}`)
    }
    const target = join(extractionRoot, expected.version)
    execFileSync('mkdir', [target])
    execFileSync('tar', ['-xzf', tarball, '-C', target])
    const root = join(target, 'package')
    const manifest = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'))
    assert.equal(manifest.name, '@earendil-works/pi-tui')
    assert.equal(manifest.version, expected.version)
    const indexJs = await readFile(join(root, 'dist', 'index.js'), 'utf8')
    const indexDts = await readFile(join(root, 'dist', 'index.d.ts'))
    assert.equal(sha256(indexJs), expected.indexJsSha256, `${expected.version} index.js`)
    assert.equal(sha256(indexDts), expected.indexDtsSha256, `${expected.version} index.d.ts`)
    const names = []
    for (const statement of indexJs.matchAll(
      /export\s+\{(?<body>[\s\S]*?)\}\s+from\s+"[^"]+";/g,
    )) {
      for (const rawName of statement.groups.body.split(',')) {
        const name = rawName.trim()
        if (name) names.push(name)
      }
    }
    assert.equal(names.length, expected.runtimeExportCount, `${expected.version} export count`)
    assert.equal(new Set(names).size, names.length, `${expected.version} duplicate export`)
    exportsByVersion.set(expected.version, names)
  }

  assert.deepEqual(
    exportsByVersion.get(drift.baseline.version),
    oracle.canonicalRuntimeSourceOrder,
    '0.84.2 keeps the pinned 0.84.1 runtime namespace',
  )
  const baseline = new Set(exportsByVersion.get(drift.baseline.version))
  const latest = exportsByVersion.get(drift.observedLatest.version)
  assert.deepEqual(
    latest.filter((name) => !baseline.has(name)),
    drift.observedLatest.addedRuntimeExports,
    'observed latest drift remains the recorded one-export delta',
  )
  console.log('upstream drift OK: 0.84.2 keeps 69 exports; 0.84.4 adds setCapabilityOverrides')
} finally {
  await rm(extractionRoot, { recursive: true, force: true })
}
