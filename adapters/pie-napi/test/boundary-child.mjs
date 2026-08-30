import assert from 'node:assert/strict'
import { Buffer } from 'node:buffer'

import { encodeKitty, renderLatex, truncateToWidth } from '../index.js'

function canonicalEncodeKitty(base64Data) {
  const params = 'a=T,f=100,q=2'
  if (base64Data.length <= 4096) {
    return `\u001b_G${params};${base64Data}\u001b\\`
  }
  const chunks = []
  for (let offset = 0; offset < base64Data.length; offset += 4096) {
    const chunk = base64Data.slice(offset, offset + 4096)
    const isLast = offset + 4096 >= base64Data.length
    chunks.push(
      offset === 0
        ? `\u001b_G${params},m=1;${chunk}\u001b\\`
        : `\u001b_Gm=${isLast ? 0 : 1};${chunk}\u001b\\`,
    )
  }
  return chunks.join('')
}

for (const input of [
  'A'.repeat(4095),
  'A'.repeat(4096),
  'A'.repeat(4097),
  'é'.repeat(4095),
  'é'.repeat(4096),
  'é'.repeat(4097),
  `${'😀'.repeat(2047)}x`,
  '😀'.repeat(2048),
  `${'x'.repeat(4095)}😀`,
]) {
  assert.ok(
    Buffer.from(encodeKitty(input), 'utf16le').equals(
      Buffer.from(canonicalEncodeKitty(input), 'utf16le'),
    ),
  )
}

const paddedWidth = Number(process.env.PIE_NAPI_PAD_WIDTH ?? 0xffffffff)
assert.throws(
  () => truncateToWidth('', paddedWidth, '...', true),
  RangeError,
)
assert.equal(renderLatex('x'), 'x')
console.log('boundary child OK')
