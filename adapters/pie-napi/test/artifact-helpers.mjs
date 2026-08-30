import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'

export function findNativeArtifact(packageRoot) {
  const artifacts = readdirSync(packageRoot)
    .filter((name) => name.endsWith('.node'))
    .map((name) => join(packageRoot, name))
  assert.equal(artifacts.length, 1, 'exactly one host native artifact')
  return artifacts[0]
}

function command(commandName, args) {
  return execFileSync(commandName, args, {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
}

export function inspectNativeArtifact(binary, forbiddenPrefixes = []) {
  const strings = command('strings', ['-a', binary])
  const prefixes = new Set([
    ...forbiddenPrefixes,
    homedir(),
    tmpdir(),
  ])
  for (const prefix of prefixes) {
    if (prefix && strings.includes(prefix)) {
      throw new Error('native artifact contains a forbidden build prefix')
    }
  }

  const personalMarkers = [
    `/${['U', 'sers'].join('')}/`,
    `/${['h', 'ome'].join('')}/`,
    `${['C', ':'].join('')}\\${['U', 'sers'].join('')}\\`,
  ]
  for (const marker of personalMarkers) {
    if (strings.includes(marker)) {
      throw new Error('native artifact contains a personal path marker')
    }
  }
  if (
    strings.includes(`${['.', 'cargo'].join('')}/registry`) ||
    strings.includes(`${['.', 'rustup'].join('')}/toolchains`)
  ) {
    throw new Error('native artifact contains an unremapped toolchain path')
  }

  const fileType = command('file', [binary])
  let metadata
  if (fileType.includes('Mach-O')) {
    const commands = command('otool', ['-l', binary])
    const id = /cmd LC_ID_DYLIB[\s\S]*?\n\s*name ([^\s]+) \(offset/.exec(
      commands,
    )?.[1]
    assert.equal(id, '@rpath/pie-tui-native.node', 'Mach-O LC_ID_DYLIB')
    assert.equal(id.startsWith('/'), false, 'Mach-O install name is relative')
    metadata = `mach-o:${id}`
  } else if (fileType.includes('ELF')) {
    const dynamic = command('readelf', ['-dW', binary])
    const soname = /\(SONAME\).*Library soname: \[([^\]]+)\]/.exec(
      dynamic,
    )?.[1]
    assert.equal(soname, 'pie-tui-native.node', 'ELF SONAME')
    assert.equal(/\((?:RPATH|RUNPATH)\)/.test(dynamic), false, 'ELF has no rpath')
    metadata = `elf:${soname}`
  } else {
    throw new Error(`unsupported native artifact metadata: ${fileType.trim()}`)
  }

  const bytes = readFileSync(binary)
  return {
    bytes,
    metadata,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  }
}
