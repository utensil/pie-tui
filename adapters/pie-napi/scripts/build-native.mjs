import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const workspaceRoot = resolve(packageRoot, '..', '..')
const require = createRequire(import.meta.url)

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: packageRoot,
    encoding: 'utf8',
    ...options,
  })
  if (result.error) throw result.error
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? '')
    process.stderr.write(result.stderr ?? '')
    throw new Error(`${command} exited with status ${result.status}`)
  }
  return (result.stdout ?? '').trim()
}

const rustup = process.env.RUSTUP ?? 'rustup'
const cargo = run(rustup, ['which', 'cargo', '--toolchain', '1.98.0'])
const rustc = run(rustup, ['which', 'rustc', '--toolchain', '1.98.0'])
const rustdoc = run(rustup, ['which', 'rustdoc', '--toolchain', '1.98.0'])
assertPinnedCompiler()

function assertPinnedCompiler() {
  const version = run(rustc, ['--version'])
  if (!version.startsWith('rustc 1.98.0 ')) {
    throw new Error(`native build requires Rust 1.98.0, got ${version}`)
  }
}

const userHome = homedir()
const cargoHome = resolve(process.env.CARGO_HOME ?? join(userHome, '.cargo'))
const sysroot = run(rustc, ['--print', 'sysroot'])
const remaps = [
  [workspaceRoot, 'pie-tui-source'],
  [cargoHome, 'cargo-home'],
  [sysroot, 'rust-sysroot'],
  [userHome, 'build-home'],
]
  .filter(
    ([source], index, entries) =>
      source.length > 0 &&
      entries.findIndex(([candidate]) => candidate === source) === index,
  )
  // rustc applies the last matching remap, so broad home comes first and the
  // workspace/cargo/sysroot-specific identities win.
  .sort(([left], [right]) => left.length - right.length)
  .map(([source, replacement]) => `--remap-path-prefix=${source}=${replacement}`)

const environment = {
  ...process.env,
  CARGO: cargo,
  RUSTC: rustc,
  RUSTDOC: rustdoc,
  RUSTUP_TOOLCHAIN: '1.98.0',
  CARGO_INCREMENTAL: '0',
  CARGO_ENCODED_RUSTFLAGS: remaps.join('\u001f'),
  SOURCE_DATE_EPOCH: '0',
  ZERO_AR_DATE: '1',
}
delete environment.RUSTFLAGS
delete environment.CARGO_BUILD_RUSTFLAGS

const cliPackage = require.resolve('@napi-rs/cli/package.json')
const cli = join(dirname(cliPackage), 'dist', 'cli.js')
const args = [
  cli,
  'build',
  '--platform',
  '--release',
  '--js',
  'native-loader.cjs',
  '--dts',
  'native.d.ts',
  '--manifest-path',
  'Cargo.toml',
]
if (process.env.PIE_NAPI_TARGET_DIR) {
  args.push('--target-dir', resolve(process.env.PIE_NAPI_TARGET_DIR))
}
args.push('--', '--locked')

run(process.execPath, args, {
  env: environment,
  stdio: 'inherit',
})
