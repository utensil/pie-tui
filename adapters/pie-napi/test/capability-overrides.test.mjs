import assert from 'node:assert/strict'
import test from 'node:test'

import * as api from '../index.js'
import fixture from './oracle-contract.json' with { type: 'json' }

const environmentKeys = [
  ...fixture.capabilityEnvironmentKeys,
  'PI_HYPERLINKS',
  'PI_IMAGE_PROTOCOL',
  'PI_TRUE_COLOR',
]

function withUnknownTrueColorTerminal(run) {
  const previous = new Map(environmentKeys.map((key) => [key, process.env[key]]))
  try {
    for (const key of environmentKeys) delete process.env[key]
    process.env.COLORTERM = 'truecolor'
    api.setCapabilityOverrides({})
    api.resetCapabilitiesCache()
    return run()
  } finally {
    api.setCapabilityOverrides({})
    api.resetCapabilitiesCache()
    for (const key of environmentKeys) delete process.env[key]
    for (const [key, value] of previous) {
      if (value !== undefined) process.env[key] = value
    }
  }
}

test('capability overrides clone the input and take partial precedence', () => {
  withUnknownTrueColorTerminal(() => {
    const overrides = { images: 'kitty' }
    api.setCapabilityOverrides(overrides)
    overrides.images = null

    const detected = api.getCapabilities()
    assert.deepEqual(detected, {
      images: 'kitty',
      trueColor: true,
      hyperlinks: false,
    })
    assert.notEqual(detected, overrides)
  })
})

test('equal overrides preserve cache identity while changed values invalidate it', () => {
  withUnknownTrueColorTerminal(() => {
    api.setCapabilityOverrides({ images: 'kitty', hyperlinks: true })
    const first = api.getCapabilities()

    api.setCapabilityOverrides({ images: 'kitty', hyperlinks: true })
    assert.equal(api.getCapabilities(), first)

    api.setCapabilityOverrides({ images: 'kitty', hyperlinks: false })
    const changed = api.getCapabilities()
    assert.notEqual(changed, first)
    assert.deepEqual(changed, {
      images: 'kitty',
      trueColor: true,
      hyperlinks: false,
    })
  })
})

test('persistent overrides survive reset and resume after setCapabilities', () => {
  withUnknownTrueColorTerminal(() => {
    api.setCapabilityOverrides({ trueColor: false })
    const temporary = { images: 'iterm2', trueColor: true, hyperlinks: true }
    api.setCapabilities(temporary)
    assert.equal(api.getCapabilities(), temporary)

    api.resetCapabilitiesCache()
    assert.deepEqual(api.getCapabilities(), {
      images: null,
      trueColor: false,
      hyperlinks: false,
    })
  })
})

test('empty overrides restore detection and null is rejected', () => {
  withUnknownTrueColorTerminal(() => {
    api.setCapabilityOverrides({ images: 'kitty', trueColor: false })
    assert.equal(api.getCapabilities().images, 'kitty')

    api.setCapabilityOverrides({})
    assert.deepEqual(api.getCapabilities(), {
      images: null,
      trueColor: true,
      hyperlinks: false,
    })
    assert.throws(() => api.setCapabilityOverrides(null), TypeError)
  })
})
