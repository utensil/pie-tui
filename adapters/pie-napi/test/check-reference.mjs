import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { lstat, mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join, posix, relative, sep } from 'node:path'
import { pathToFileURL } from 'node:url'

import fixture from './oracle-contract.json' with { type: 'json' }

const distribution = process.env.PI_TUI_DIST
const tarball = process.env.PI_TUI_TARBALL
if (!distribution) {
  throw new Error('PI_TUI_DIST must point to the pinned reference dist directory')
}
if (!tarball) {
  throw new Error('PI_TUI_TARBALL must point to the pinned 0.84.2 npm tarball')
}

const expectedReference = fixture.reference

function applyEnvironment(values) {
  for (const [key, value] of Object.entries(values)) {
    if (value === null) {
      delete process.env[key]
    } else {
      process.env[key] = value
    }
  }
}

function checkCapabilitySnapshot(api, testCase) {
  const previous = new Map(
    fixture.capabilityEnvironmentKeys.map((key) => [key, process.env[key]]),
  )
  try {
    for (const key of fixture.capabilityEnvironmentKeys) delete process.env[key]
    applyEnvironment(testCase.initialEnvironment)
    let callbackCalls = 0
    const result = api.detectCapabilities(() => {
      callbackCalls += 1
      applyEnvironment(testCase.callbackMutations)
      return testCase.callbackResult
    })
    assert.deepEqual(result, testCase.result, testCase.label)
    assert.equal(callbackCalls, testCase.callbackCalls, testCase.label)
  } finally {
    for (const key of fixture.capabilityEnvironmentKeys) delete process.env[key]
    for (const [key, value] of previous) {
      if (value !== undefined) process.env[key] = value
    }
  }
}

function hashBytes(algorithm, bytes, encoding = 'hex') {
  return createHash(algorithm).update(bytes).digest(encoding)
}

async function collectTree(root) {
  const paths = []
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) {
        await visit(path)
      } else {
        const metadata = await lstat(path)
        assert.equal(metadata.isFile(), true, `non-file in dist tree: ${path}`)
        paths.push(path)
      }
    }
  }
  await visit(root)
  paths.sort((left, right) => {
    const leftName = relative(root, left).split(sep).join('/')
    const rightName = relative(root, right).split(sep).join('/')
    return leftName < rightName ? -1 : leftName > rightName ? 1 : 0
  })

  const exactNames = new Set()
  const foldedNames = new Set()
  const digest = createHash('sha256')
  const addFrame = (bytes) => {
    const length = Buffer.alloc(8)
    length.writeBigUInt64BE(BigInt(bytes.length))
    digest.update(length)
    digest.update(bytes)
  }
  for (const path of paths) {
    const name = relative(root, path).split(sep).join('/')
    const folded = name.normalize('NFC').toLowerCase()
    assert.equal(exactNames.has(name), false, `duplicate dist path: ${name}`)
    assert.equal(foldedNames.has(folded), false, `case-colliding dist path: ${name}`)
    exactNames.add(name)
    foldedNames.add(folded)
    addFrame(Buffer.from(name))
    addFrame(await readFile(path))
  }
  return {
    count: paths.length,
    digest: digest.digest('hex'),
  }
}

async function assertPackageManifest(packageRoot, label) {
  const manifest = JSON.parse(
    await readFile(join(packageRoot, 'package.json'), 'utf8'),
  )
  assert.equal(manifest.name, expectedReference.package, `${label} package name`)
  assert.equal(manifest.version, expectedReference.version, `${label} version`)
  assert.deepEqual(
    manifest.dependencies,
    expectedReference.dependencies,
    `${label} dependencies`,
  )
}

async function assertDistTree(root, label) {
  const tree = await collectTree(root)
  assert.equal(tree.count, expectedReference.distFileCount, `${label} file count`)
  assert.equal(tree.digest, expectedReference.distTreeSha256, `${label} tree digest`)
  for (const [name, expectedDigest] of Object.entries(
    expectedReference.selectedFileSha256,
  )) {
    assert.equal(
      hashBytes('sha256', await readFile(join(root, name))),
      expectedDigest,
      `${label} ${name}`,
    )
  }
}

async function findDependencyManifest(requireFromReference, dependency) {
  let directory = dirname(requireFromReference.resolve(dependency))
  for (;;) {
    const candidate = join(directory, 'package.json')
    try {
      const manifest = JSON.parse(await readFile(candidate, 'utf8'))
      if (manifest.name === dependency) {
        return { manifest, root: directory }
      }
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error
    }
    const parent = dirname(directory)
    if (parent === directory) {
      throw new Error(`could not locate ${dependency} package.json`)
    }
    directory = parent
  }
}

const artifact = await readFile(tarball)
assert.equal(
  hashBytes('sha256', artifact),
  expectedReference.tarballSha256,
  'reference tarball sha256',
)
assert.equal(
  `sha512-${hashBytes('sha512', artifact, 'base64')}`,
  expectedReference.tarballIntegrity,
  'reference tarball npm integrity',
)

const tarEntries = execFileSync('tar', ['-tzf', tarball], {
  encoding: 'utf8',
  maxBuffer: 8 * 1024 * 1024,
})
  .split('\n')
  .filter(Boolean)
const distTarEntries = tarEntries.filter(
  (name) => name.startsWith('package/dist/') && !name.endsWith('/'),
)
assert.equal(
  distTarEntries.length,
  expectedReference.distFileCount,
  'tarball dist entry count',
)
const tarExactNames = new Set()
const tarFoldedNames = new Set()
for (const name of distTarEntries) {
  assert.equal(posix.isAbsolute(name), false, `absolute tar entry: ${name}`)
  assert.equal(posix.normalize(name), name, `unsafe tar entry: ${name}`)
  assert.equal(tarExactNames.has(name), false, `duplicate tar entry: ${name}`)
  const folded = name.normalize('NFC').toLowerCase()
  assert.equal(
    tarFoldedNames.has(folded),
    false,
    `case-colliding tar entry: ${name}`,
  )
  tarExactNames.add(name)
  tarFoldedNames.add(folded)
}

const extractionRoot = await mkdtemp(join(tmpdir(), 'pie-tui-reference-check-'))
try {
  execFileSync('tar', ['-xzf', tarball, '-C', extractionRoot], {
    stdio: 'pipe',
  })
  const extractedPackage = join(extractionRoot, 'package')
  const extractedDist = join(extractedPackage, 'dist')
  await assertPackageManifest(extractedPackage, 'tarball')
  await assertDistTree(extractedDist, 'tarball dist')

  const colocatedPackage = dirname(distribution)
  await assertPackageManifest(colocatedPackage, 'colocated install')
  await assertDistTree(distribution, 'colocated dist')

  const requireFromReference = createRequire(
    pathToFileURL(join(distribution, 'index.js')),
  )
  for (const [dependency, expectedVersion] of Object.entries(
    expectedReference.dependencies,
  )) {
    const { manifest, root } = await findDependencyManifest(
      requireFromReference,
      dependency,
    )
    assert.equal(manifest.version, expectedVersion, `${dependency} installed version`)
    const tree = await collectTree(root)
    const expectedTree = expectedReference.dependencyTrees[dependency]
    assert.equal(tree.count, expectedTree.fileCount, `${dependency} file count`)
    assert.equal(tree.digest, expectedTree.treeSha256, `${dependency} tree digest`)
  }

const declaration = await readFile(join(distribution, 'index.d.ts'))
assert.equal(
  createHash('sha256').update(declaration).digest('hex'),
  fixture.reference.selectedFileSha256['index.d.ts'],
)

for (const [name, expectedDigest] of Object.entries(
  expectedReference.selectedSurfaceClosureSha256,
)) {
  assert.equal(
    hashBytes('sha256', await readFile(join(distribution, name))),
    expectedDigest,
    `selected surface closure ${name}`,
  )
}

const indexSource = await readFile(join(distribution, 'index.js'), 'utf8')
const canonicalSourceOrder = []
for (const statement of indexSource.matchAll(
  /export\s+\{(?<body>[\s\S]*?)\}\s+from\s+"[^"]+";/g,
)) {
  for (const rawName of statement.groups.body.split(',')) {
    const name = rawName.trim()
    if (name) canonicalSourceOrder.push(name)
  }
}
assert.deepEqual(
  canonicalSourceOrder,
  fixture.canonicalRuntimeSourceOrder,
  'canonical index.js runtime export source order',
)

const reference = await import(pathToFileURL(join(distribution, 'index.js')))
assert.deepEqual(
  Object.keys(reference),
  Object.keys(fixture.canonicalRuntimeTypes),
  'canonical runtime namespace order/count',
)
for (const [name, expectedType] of Object.entries(fixture.canonicalRuntimeTypes)) {
  assert.equal(typeof reference[name], expectedType, name)
  const descriptor = Object.getOwnPropertyDescriptor(reference, name)
  assert.ok(Object.hasOwn(descriptor, 'value'), `${name} data slot`)
  for (const [field, expected] of Object.entries(
    fixture.moduleExportDescriptor,
  )) {
    assert.equal(descriptor[field], expected, `${name} ${field}`)
  }
}
for (const [name, expected] of Object.entries(fixture.runtimeFunctions)) {
  assert.equal(reference[name].name, expected.name, `${name} name`)
  assert.equal(reference[name].length, expected.length, `${name} length`)
}

const fromUnits = (units) => String.fromCharCode(...units)
const toUnits = (value) =>
  Array.from({ length: value.length }, (_, index) => value.charCodeAt(index))

for (const testCase of fixture.latexCases) {
  assert.deepEqual(
    toUnits(reference.renderLatex(fromUnits(testCase.input))),
    testCase.output,
    testCase.label,
  )
}

for (const testCase of fixture.wrapTextWithAnsiCases) {
  assert.deepEqual(
    reference
      .wrapTextWithAnsi(fromUnits(testCase.input), testCase.width)
      .map(toUnits),
    testCase.output,
    testCase.label,
  )
}

for (const testCase of fixture.compositeTuiLineCases) {
  assert.deepEqual(
    toUnits(
      reference.compositeTuiLine(
        fromUnits(testCase.base),
        fromUnits(testCase.overlay),
        testCase.startCol,
        testCase.overlayWidth,
        testCase.totalWidth,
      ),
    ),
    testCase.output,
    testCase.label,
  )
}

assert.equal(reference.renderLatex('\\definitelyUnsupported{x}'), undefined)
assert.equal(reference.getPngDimensions(''), null)
assert.equal(reference.parseOsc11BackgroundColor('bad'), undefined)
assert.deepEqual(Object.keys(reference.Key), fixture.keyKeys)
for (const name of fixture.keyKeys) {
  const descriptor = Object.getOwnPropertyDescriptor(reference.Key, name)
  for (const [field, expected] of Object.entries(
    fixture.keyPropertyDescriptor,
  )) {
    assert.equal(descriptor[field], expected, `Key.${name} ${field}`)
  }
}
for (const [name, expected] of Object.entries(fixture.keyHelperFunctions)) {
  assert.equal(reference.Key[name].name, expected.name, `Key.${name} name`)
  assert.equal(reference.Key[name].length, expected.length, `Key.${name} length`)
}

assert.equal(
  reference.encodeKitty('QQ==', { imageId: 0 }),
  '\u001b_Ga=T,f=100,q=2;QQ==\u001b\\',
)
for (const testCase of fixture.kittyUtf16Cases) {
  let input
  if (testCase.family === 'ascii') {
    input = 'A'.repeat(testCase.length)
  } else if (testCase.family === 'bmp') {
    input = 'é'.repeat(testCase.length)
  } else if (testCase.family === 'astral') {
    input =
      '😀'.repeat(Math.floor(testCase.length / 2)) +
      (testCase.length % 2 === 0 ? '' : 'x')
  } else {
    input = `${'x'.repeat(testCase.length - 2)}😀`
  }
  assert.equal(input.length, testCase.length, testCase.label)
  assert.equal(
    createHash('sha256')
      .update(Buffer.from(reference.encodeKitty(input), 'utf16le'))
      .digest('hex'),
    testCase.outputUtf16leSha256,
    testCase.label,
  )
}
for (const [value, size] of [
  ['Q', 0],
  ['Q==', 0],
  ['\n', 0],
  ['é', 0],
  ['中', 0],
  ['😀', 1],
  ['💩QQ==', 3],
]) {
  assert.equal(
    reference.encodeITerm2(value),
    `\u001b]1337;File=inline=1;size=${size}:${value}\u0007`,
    JSON.stringify(value),
  )
}
assert.equal(
  reference.deleteKittyImage(0),
  '\u001b_Ga=d,d=I,i=0,q=2\u001b\\',
)
// The reference interpolates wider/coercive JS values. The native facade
// intentionally documents and guards the narrower lossless u32 ABI domain.
assert.match(reference.deleteKittyImage(-1), /i=-1/)
assert.match(reference.deleteKittyImage(1.5), /i=1\.5/)
assert.match(reference.deleteKittyImage(0x100000000), /i=4294967296/)

for (const [label, args, expected] of [
  ['negative target', [{ widthPx: 10, heightPx: 20 }, -3], 1],
  ['fractional target', [{ widthPx: 10, heightPx: 20 }, 2.9], 2],
  [
    'above u32 target',
    [{ widthPx: 10, heightPx: 20 }, 4294967296],
    4294967296,
  ],
  ['string target', [{ widthPx: 10, heightPx: 20 }, '2.9'], 2],
  ['string fields', [{ widthPx: '10', heightPx: '20' }, 2], 2],
  ['null fields', [{ widthPx: null, heightPx: null }, 2], 1],
]) {
  assert.equal(Reflect.apply(reference.calculateImageRows, null, args), expected, label)
}
for (const [label, args] of [
  ['NaN target', [{ widthPx: 10, heightPx: 20 }, NaN]],
  ['missing target', [{ widthPx: 10, heightPx: 20 }]],
  ['missing width field', [{ heightPx: 20 }, 2]],
  ['missing height field', [{ widthPx: 10 }, 2]],
]) {
  assert.ok(
    Number.isNaN(Reflect.apply(reference.calculateImageRows, null, args)),
    label,
  )
}
assert.throws(
  () => reference.calculateImageRows({ widthPx: 10, heightPx: 20 }, 2, null),
  TypeError,
)

const cell = { widthPx: 7, heightPx: 13 }
reference.setCellDimensions(cell)
assert.equal(reference.getCellDimensions(), cell)
const capabilities = { images: 'kitty', trueColor: false, hyperlinks: true }
reference.setCapabilities(capabilities)
assert.equal(reference.getCapabilities(), capabilities)
for (const testCase of fixture.capabilitySnapshotCases) {
  checkCapabilitySnapshot(reference, testCase)
}

console.log(
  `oracle OK: ${fixture.reference.package}@${fixture.reference.version}, ${Object.keys(fixture.canonicalRuntimeTypes).length} canonical / ${Object.keys(fixture.runtimeTypes).length} selected runtime exports`,
)
} finally {
  await rm(extractionRoot, { recursive: true, force: true })
}
