import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdtemp, mkdir, readFile, readdir, realpath, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

import fixture from './oracle-contract.json' with { type: 'json' }
import {
  findNativeArtifact,
  inspectNativeArtifact,
} from './artifact-helpers.mjs'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const hostUsersPrefix = `/${['U', 'sers'].join('')}/`
const temporaryRoot = await mkdtemp(join(tmpdir(), 'pie-tui-native-consumer-'))
const packDirectory = join(temporaryRoot, 'pack')
const consumerDirectory = join(temporaryRoot, 'consumer')

async function collectFiles(directory) {
  const files = []
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(path)))
    } else {
      files.push(path)
    }
  }
  return files
}

try {
  const sourceArtifact = inspectNativeArtifact(
    findNativeArtifact(packageRoot),
    [packageRoot],
  )
  assert.equal(
    createHash('sha256')
      .update(await readFile(join(packageRoot, 'LICENSE')))
      .digest('hex'),
    fixture.packageLicenseSha256,
    'package LICENSE matches the repo receipt',
  )
  await mkdir(packDirectory)
  await mkdir(consumerDirectory)
  const packed = JSON.parse(
    execFileSync(
      'npm',
      ['pack', '--json', '--pack-destination', packDirectory],
      { cwd: packageRoot, encoding: 'utf8' },
    ),
  )
  assert.equal(packed.length, 1)
  assert.equal(
    packed[0].files.some((entry) => entry.path === 'LICENSE'),
    true,
    'pack manifest includes LICENSE',
  )
  const tarball = join(packDirectory, packed[0].filename)

  await writeFile(
    join(consumerDirectory, 'package.json'),
    `${JSON.stringify(
      {
        name: 'pie-tui-native-fresh-consumer',
        private: true,
        type: 'module',
        dependencies: {
          '@earendil-works/pi-tui': `file:${tarball}`,
        },
      },
      null,
      2,
    )}\n`,
  )
  execFileSync(
    'npm',
    ['install', '--ignore-scripts', '--no-audit', '--no-fund'],
    { cwd: consumerDirectory, stdio: 'pipe' },
  )

  const installedRoot = join(
    consumerDirectory,
    'node_modules',
    '@earendil-works',
    'pi-tui',
  )
  const installedRealRoot = await realpath(installedRoot)
  assert.ok(installedRealRoot.startsWith(await realpath(consumerDirectory)))
  const installedFiles = await collectFiles(installedRoot)
  assert.ok(installedFiles.some((path) => path.endsWith('.node')))
  assert.ok(installedFiles.some((path) => path.endsWith('native-loader.cjs')))
  assert.equal(
    createHash('sha256')
      .update(await readFile(join(installedRoot, 'LICENSE')))
      .digest('hex'),
    fixture.packageLicenseSha256,
    'installed alias carries the exact repo MIT license',
  )
  const installedNative = installedFiles.find((path) => path.endsWith('.node'))
  const installedArtifact = inspectNativeArtifact(installedNative, [
    packageRoot,
    temporaryRoot,
    installedRoot,
  ])
  assert.equal(installedArtifact.sha256, sourceArtifact.sha256)
  assert.equal(installedArtifact.metadata, sourceArtifact.metadata)
  const relativeFiles = installedFiles.map((path) => relative(installedRoot, path))
  assert.equal(relativeFiles.some((path) => path.endsWith('.rs')), false)
  assert.equal(relativeFiles.some((path) => path.startsWith('src/')), false)
  assert.equal(relativeFiles.includes('Cargo.toml'), false)
  assert.equal(relativeFiles.some((path) => path.startsWith('test/')), false)

  const installedManifest = JSON.parse(
    await readFile(join(installedRoot, 'package.json'), 'utf8'),
  )
  assert.equal(installedManifest.name, 'pie-tui-native')
  assert.equal(installedManifest.private, true)
  assert.equal(installedManifest.license, 'MIT')
  assert.equal(installedManifest.engines.node, '>=24.4.1')
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:m6oracle/,
    'verify command includes the authenticated 0.84.2 M6 semantic gate',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:tier1oracle/,
    'verify command includes the authenticated 0.84.2 Tier-1 scroll gate',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:selectionoracle/,
    'verify command includes the authenticated selection geometry gate',
  )
  assert.equal(
    installedManifest.scripts['test:wordcopyoracle'],
    'node test/word-copy-oracle.mjs',
    'package preserves the standalone authenticated word\/copy gate',
  )
  assert.doesNotMatch(
    installedManifest.scripts.verify,
    /npm run test:wordcopyoracle/,
    'aggregate verify intentionally leaves the standalone word\/copy gate out',
  )
  assert.equal(
    installedManifest.scripts['test:autosearchoracle'],
    'node test/auto-scroll-search-oracle.mjs',
    'package preserves the standalone authenticated auto-scroll/search gate',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:autosearchoracle/,
    'verify command includes the authenticated auto-scroll/search gate after implementation',
  )
  assert.equal(
    installedManifest.scripts['test:searchgraphemeoracle'],
    'node test/search-grapheme-oracle.mjs',
    'package preserves the standalone authenticated search-grapheme gate',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:searchgraphemeoracle/,
    'verify command includes the authenticated search-grapheme gate after implementation',
  )
  assert.equal(
    installedManifest.scripts['test:searchhighlightoracle'],
    'node test/search-highlight-oracle.mjs',
    'package preserves the standalone authenticated search-highlight gate',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run test:searchhighlightoracle/,
    'verify command includes the authenticated search-highlight gate after implementation',
  )
  assert.match(
    installedManifest.scripts.verify,
    /npm run oracle:overlay/,
    'verify command includes the authenticated 0.84.4 overlay gate',
  )
  const installedReadme = await readFile(join(installedRoot, 'README.md'), 'utf8')
  assert.match(
    installedReadme,
    /complete 69-export runtime namespace and exact 133-name baseline\nof the authenticated pi-tui 0\.84\.2 package/,
    'README pins the complete 69/133 baseline',
  )
  assert.match(
    installedReadme,
    /actual 70-export \/ 134-name facade/,
    'README pins the one-export adopted overlay',
  )
  assert.match(
    installedReadme,
    /it is not the upstream package and does not claim compatibility outside the\ndocumented parity ledger/,
    'README preserves the recorded compatibility boundary',
  )
  assert.match(
    installedReadme,
    /Thin JavaScript facades retain the\nupstream object graph, callback, timer, process-stream/,
    'README records the native and JavaScript ownership seam',
  )

  for (const path of installedFiles.filter((file) =>
    /\.(?:cjs|js|json|md|ts)$/.test(file),
  )) {
    const content = await readFile(path, 'utf8')
    assert.equal(content.includes(packageRoot), false, path)
    assert.equal(content.includes(hostUsersPrefix), false, path)
  }

  await writeFile(
    join(consumerDirectory, 'probe.mjs'),
    `
      import assert from 'node:assert/strict'
      import Module, { createRequire } from 'node:module'
      const require = createRequire(import.meta.url)
      const nativeLoads = []
      const originalLoad = Module._load
      Module._load = function (request, parent, isMain) {
        const loaded = originalLoad.call(this, request, parent, isMain)
        if (typeof request === 'string' && request.endsWith('.node')) {
          nativeLoads.push({ request, parent: parent?.filename })
        }
        return loaded
      }
      const esm = await import('@earendil-works/pi-tui')
      const cjs = require('@earendil-works/pi-tui')
      Module._load = originalLoad
      const expected = ${JSON.stringify(Object.keys(fixture.runtimeTypes).sort())}
      assert.deepEqual(Object.keys(esm).sort(), expected)
      assert.deepEqual(Object.keys(cjs).sort(), expected)
      assert.equal(nativeLoads.length, 1)
      assert.ok(nativeLoads[0].request.startsWith('./pie-tui-native.'))
      assert.ok(nativeLoads[0].request.endsWith('.node'))
      assert.ok(nativeLoads[0].parent.startsWith(${JSON.stringify(installedRealRoot)}))
      assert.equal(esm.renderLatex('x'), 'x')
      assert.equal(cjs.renderLatex('x'), 'x')
      assert.equal(esm.CURSOR_MARKER, String.fromCharCode(27) + '_pi:c' + String.fromCharCode(7))
      assert.equal(cjs.TUI_KEYBINDINGS, esm.TUI_KEYBINDINGS)
      assert.equal(Object.keys(esm.TUI_KEYBINDINGS).length, 47)
      assert.deepEqual(esm.fuzzyMatch('fb', 'fooBar'), { matches: true, score: -10.7 })
      assert.deepEqual(esm.fuzzyFilter([{ text: 'fooBar' }], 'fb', (item) => item.text), [{ text: 'fooBar' }])
      const spacer = new esm.Spacer(2)
      assert.equal(spacer instanceof cjs.Spacer, true)
      assert.deepEqual(spacer.render(80), ['', ''])
      assert.match(esm.compositeTuiLine('abcd', 'X', 1, 1, 4), /X/)
      esm.setCapabilities({ images: 'kitty', trueColor: true, hyperlinks: true })
      assert.match(esm.renderImage('QQ==', { widthPx: 1, heightPx: 1 }).sequence, /^\\u001b_G/)
      assert.match(esm.imageFallback('image/png', { widthPx: 1, heightPx: 1 }), /1x1/)
      esm.setCapabilityOverrides({ images: 'iterm2' })
      esm.resetCapabilitiesCache()
      assert.equal(esm.getCapabilities().images, 'iterm2')
      esm.setCapabilityOverrides({})
      esm.resetCapabilitiesCache()
      assert.deepEqual(
        Array.from(esm.renderLatex(String.fromCharCode(92, 98, 97, 114, 32, 0xd83d)), (unit) => unit.charCodeAt(0)),
        [0xd83d, 773],
      )
      assert.equal(esm.encodeKitty('QQ==', { imageId: 0 }).includes('i='), false)
      assert.match(esm.deleteKittyImage(0), /i=0/)
      const cell = { widthPx: 4, heightPx: 8 }
      esm.setCellDimensions(cell)
      assert.equal(cjs.getCellDimensions(), cell)
      const input = new esm.Input()
      input.setValue('m5')
      input.handleInput('!')
      assert.equal(input.getValue(), '!m5')
      const container = new esm.Container()
      const text = new esm.Text('consumer')
      container.addChild(text)
      assert.equal(container.children[0], text)
      assert.ok(container.render(20).some((line) => line.includes('consumer')))
      const terminal = {
        starts: 0,
        stops: 0,
        writes: [],
        columns: 20,
        rows: 5,
        kittyProtocolActive: false,
        start(onInput, onResize) { this.starts += 1; this.onInput = onInput; this.onResize = onResize },
        stop() { this.stops += 1 },
        async drainInput() {},
        write(value) { this.writes.push(value) },
        moveBy() {}, hideCursor() {}, showCursor() {}, clearLine() {},
        clearFromCursor() {}, clearScreen() {}, setTitle() {}, setProgress() {},
      }
      const main = new esm.TuiMainScreen(terminal, false)
      main.addChild(text)
      main.start()
      main.renderNow(true)
      main.stop()
      assert.equal(terminal.starts, 1)
      assert.equal(terminal.stops, 1)
      console.log('fresh alias consumer OK')
    `,
  )
  const probe = execFileSync('node', ['probe.mjs'], {
    cwd: consumerDirectory,
    encoding: 'utf8',
    env: { ...process.env, NAPI_RS_NATIVE_LIBRARY_PATH: '' },
  })
  assert.match(probe, /fresh alias consumer OK/)
  console.log(
    `pack alias consumer OK (full recorded runtime contract): ${packed[0].filename}, ${installedFiles.length} installed files`,
  )
} finally {
  await rm(temporaryRoot, { recursive: true, force: true })
}
