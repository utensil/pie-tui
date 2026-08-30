import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readlink,
  realpath,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { homedir, tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import {
  findNativeArtifact,
  inspectNativeArtifact,
} from './artifact-helpers.mjs'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const workspaceRoot = resolve(packageRoot, '..', '..')
const originalNodeModules = join(packageRoot, 'node_modules')
const shortRoot = await mkdtemp(join(tmpdir(), 'pie-napi-repro-a-'))
const longRoot = await mkdtemp(
  join(tmpdir(), 'pie-napi-repro-significantly-long-root-b-'),
)

async function copyWorkspace(destination) {
  const names = execFileSync(
    'git',
    ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
    { cwd: workspaceRoot },
  )
    .toString()
    .split('\0')
    .filter(Boolean)
  for (const name of names) {
    const source = join(workspaceRoot, name)
    const target = join(destination, name)
    const metadata = await lstat(source)
    await mkdir(dirname(target), { recursive: true })
    if (metadata.isSymbolicLink()) {
      await symlink(await readlink(source), target)
    } else {
      assert.equal(metadata.isFile(), true, `unsupported workspace entry: ${name}`)
      await copyFile(source, target)
      await chmod(target, metadata.mode)
    }
  }
  await symlink(
    originalNodeModules,
    join(destination, 'adapters', 'pie-napi', 'node_modules'),
  )
}

function runBuild(root) {
  const copiedPackage = join(root, 'adapters', 'pie-napi')
  const result = spawnSync(process.execPath, ['scripts/build-native.mjs'], {
    cwd: copiedPackage,
    encoding: 'utf8',
    env: {
      ...process.env,
      NAPI_RS_NATIVE_LIBRARY_PATH: '',
      PIE_NAPI_TARGET_DIR: join(root, 'target'),
    },
    maxBuffer: 64 * 1024 * 1024,
  })
  if (result.error) throw result.error
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? '')
    process.stderr.write(result.stderr ?? '')
    throw new Error(`isolated native build exited with ${result.status}`)
  }
  return findNativeArtifact(copiedPackage)
}

async function replaceOnce(path, from, to) {
  const content = await readFile(path, 'utf8')
  assert.equal(content.split(from).length - 1, 1, 'artifact mutation marker')
  await writeFile(path, content.replace(from, to))
}

function assertMutationRejected(name, binary, forbiddenPrefixes, expected) {
  let rejection
  try {
    inspectNativeArtifact(binary, forbiddenPrefixes)
  } catch (error) {
    rejection = error
  }
  assert.ok(rejection, `artifact mutation survived: ${name}`)
  assert.match(String(rejection.message), expected)
  console.log(`artifact mutation killed: ${name}`)
}

try {
  await copyWorkspace(shortRoot)
  await copyWorkspace(longRoot)
  const forbiddenPrefixes = [
    workspaceRoot,
    await realpath(workspaceRoot),
    shortRoot,
    await realpath(shortRoot),
    longRoot,
    await realpath(longRoot),
    homedir(),
  ]

  const shortBinary = runBuild(shortRoot)
  const longBinary = runBuild(longRoot)
  const shortReceipt = inspectNativeArtifact(
    shortBinary,
    forbiddenPrefixes,
  )
  const longReceipt = inspectNativeArtifact(longBinary, forbiddenPrefixes)
  assert.equal(shortReceipt.metadata, longReceipt.metadata)
  assert.equal(shortReceipt.sha256, longReceipt.sha256)
  assert.ok(shortReceipt.bytes.equals(longReceipt.bytes), 'native bytes differ')
  console.log(
    `reproducible native artifact OK: ${shortReceipt.sha256}, ${shortReceipt.metadata}`,
  )

  const mutatedBuildScript = join(
    shortRoot,
    'adapters',
    'pie-napi',
    'scripts',
    'build-native.mjs',
  )
  await replaceOnce(
    mutatedBuildScript,
    "  CARGO_ENCODED_RUSTFLAGS: remaps.join('\\u001f'),\n",
    "  CARGO_ENCODED_RUSTFLAGS: '',\n",
  )
  const unremappedBinary = runBuild(shortRoot)
  assertMutationRejected(
    'path-remap-pipeline',
    unremappedBinary,
    forbiddenPrefixes,
    /forbidden build prefix/,
  )

  const mutatedBuildRs = join(
    longRoot,
    'adapters',
    'pie-napi',
    'build.rs',
  )
  if (process.platform === 'darwin') {
    await replaceOnce(
      mutatedBuildRs,
      '"cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/pie-tui-native.node"',
      '"cargo:warning=install-name mutation"',
    )
  } else if (process.platform === 'linux') {
    await replaceOnce(
      mutatedBuildRs,
      '"cargo:rustc-cdylib-link-arg=-Wl,-soname,pie-tui-native.node"',
      '"cargo:warning=soname mutation"',
    )
  } else {
    throw new Error(`artifact metadata mutation unsupported on ${process.platform}`)
  }
  const identityMutationBinary = runBuild(longRoot)
  assertMutationRejected(
    'native-library-identity',
    identityMutationBinary,
    forbiddenPrefixes,
    process.platform === 'darwin' ? /LC_ID_DYLIB/ : /ELF SONAME/,
  )
} finally {
  await rm(shortRoot, { recursive: true, force: true })
  await rm(longRoot, { recursive: true, force: true })
}
