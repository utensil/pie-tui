import assert from 'node:assert/strict'
import test from 'node:test'
import * as api from '../index.js'
import {
  assertPublicNegativeWidthGuards, narrowDialogScenarios,
} from './narrow-dialogs-scenarios.mjs'

for (const row of narrowDialogScenarios(api)) {
  test(`narrow dialogs ${row.kind} at width ${row.width}`, () => {
    assert.equal(row.error, undefined, JSON.stringify(row.error))
    assert.ok(row.before.length > 0)
    assert.ok(row.after.length > 0)
  })
}

test('narrow dialogs retain public negative-width guards', () => {
  assertPublicNegativeWidthGuards(api)
})
