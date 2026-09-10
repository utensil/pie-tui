import assert from 'node:assert/strict'

export const narrowDialogWidths = [0, 1, 2, 3, 4, 24, 90]

export function narrowDialogScenarios(api) {
  const identity = (value) => value
  const settingsTheme = {
    cursor: '→ ', label: identity, value: identity,
    description: identity, hint: identity,
  }
  const selectTheme = {
    selectedPrefix: () => '→ ', selectedText: identity,
    description: identity, scrollInfo: identity, noMatch: identity,
  }
  const results = []
  for (const width of narrowDialogWidths) {
    for (const kind of [
      'settings-value', 'settings-scroll-description',
      'select-primary', 'select-scroll', 'select-custom',
    ]) {
      const callbacks = []
      const events = []
      const settingItems = [
        {
          id: 'thinking', label: 'Show thinking blocks', currentValue: 'yes',
          values: ['yes', 'no'],
          description: kind === 'settings-scroll-description'
            ? 'Show details while working' : undefined,
        },
        { id: 'compact', label: 'Auto-compact', currentValue: 'true', values: ['true', 'false'] },
      ]
      const selectItems = [
        { value: 'model', label: 'Choose a model', description: 'Description' },
        { value: 'settings', label: 'Open settings', description: 'Settings description' },
      ]
      const settings = kind.startsWith('settings')
      const component = settings
        ? new api.SettingsList(
          kind === 'settings-value' ? settingItems.slice(0, 1) : settingItems,
          1, settingsTheme,
          (id, value) => events.push({ id, value }),
          () => events.push({ cancelled: true }),
        )
        : new api.SelectList(
          selectItems, kind === 'select-scroll' ? 1 : 2, selectTheme,
          kind === 'select-custom' ? {
            truncatePrimary(args) {
              callbacks.push({
                ...args,
                sameItem: args.item === selectItems.find((item) => item.value === args.item.value),
              })
              return args.text
            },
          } : {},
        )
      const result = { kind, width, callbacks, events }
      try {
        result.before = component.render(width)
        component.handleInput(settings ? ' ' : '\u001b[B')
        result.after = component.render(width)
        if (settings) {
          component.handleInput('\u001b')
          assert.deepEqual(events, [{ id: 'thinking', value: 'no' }, { cancelled: true }])
        } else {
          assert.equal(component.getSelectedItem().value, 'settings')
        }
        if (kind === 'select-custom') {
          assert.equal(callbacks.length, 4)
          assert.ok(callbacks.every((call) => call.sameItem))
          if (width <= 24) {
            assert.ok(callbacks.every((call) =>
              call.maxWidth === width - 4 && call.columnWidth === width - 4))
          }
          assert.deepEqual(callbacks.map((call) => call.isSelected), [true, false, false, true])
        }
      } catch (error) {
        result.error = { name: error.name, message: error.message }
      }
      results.push(result)
    }
  }
  return results
}

export function assertPublicNegativeWidthGuards(api) {
  for (const name of ['truncateToWidth', 'wrapTextWithAnsi']) {
    assert.throws(() => api[name]('text', -1), {
      name: 'RangeError', message: /unsigned 32-bit integer/,
    }, `${name} preserves its public negative-width guard`)
  }
}
