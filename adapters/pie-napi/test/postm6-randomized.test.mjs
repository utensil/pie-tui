import assert from 'node:assert/strict'
import test from 'node:test'

import {
  ScrollView,
  Text,
  TuiAltScreen,
  VStack,
} from '../index.js'

class RecordingTerminal {
  columns = 24
  rows = 8
  kittyProtocolActive = false
  events = []

  start(onInput, onResize) { this.onInput = onInput; this.onResize = onResize }
  stop() {}
  async drainInput() {}
  write(data) { this.events.push(data) }
  moveBy() {}
  hideCursor() {}
  showCursor() {}
  clearLine() {}
  clearFromCursor() {}
  clearScreen() {}
  setTitle() {}
  setProgress() {}
}

function parseSeeds() {
  const replay = process.env.PIE_POSTM6_SEED
  if (replay === undefined) return [0x13579bdf, 0x2468ace1, 0xc001d00d]
  const seed = Number(replay)
  assert.ok(Number.isInteger(seed) && seed > 0 && seed <= 0xffffffff,
    'PIE_POSTM6_SEED must be a nonzero uint32 (decimal or 0x-prefixed)')
  return [seed >>> 0]
}

function makePrng(seed) {
  let state = seed >>> 0
  return (limit) => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    state >>>= 0
    return state % limit
  }
}

function maximum(model) {
  return Math.max(0, model.contentHeight - model.viewportHeight)
}

function updateLayout(model, contentHeight, viewportHeight) {
  model.contentHeight = contentHeight
  model.viewportHeight = viewportHeight
  const end = maximum(model)
  if (model.followingEnd) model.scrollTop = end
  else model.scrollTop = Math.max(0, Math.min(model.scrollTop, end))
  if (model.scrollTop < end) model.followSuppressedAtEnd = false
  if (model.scrollTop === end && !model.followSuppressedAtEnd) {
    model.followingEnd = true
  }
}

function scrollTo(model, requested, disableFollow) {
  const end = maximum(model)
  const next = Math.max(0, Math.min(end, Math.trunc(requested)))
  const nextSuppressed = disableFollow && next === end
  const nextFollowing = !nextSuppressed && next === end
  const changed = next !== model.scrollTop ||
    nextFollowing !== model.followingEnd ||
    nextSuppressed !== model.followSuppressedAtEnd
  model.scrollTop = next
  model.followingEnd = nextFollowing
  model.followSuppressedAtEnd = nextSuppressed
  return Number(changed)
}

function scrollBy(model, requested) {
  const amount = Math.trunc(requested)
  if (amount === 0) return 0
  const end = maximum(model)
  const start = model.followingEnd ? end : model.scrollTop
  const next = Math.max(0, Math.min(end, start + amount))
  const moved = next - start
  const wasFollowing = model.followingEnd
  model.scrollTop = next
  model.followingEnd = next === end
  model.followSuppressedAtEnd = false
  return Number(moved !== 0 || model.followingEnd !== wasFollowing)
}

function scrollToStart(model) {
  const nextFollowing = model.contentHeight <= model.viewportHeight
  const changed = model.scrollTop !== 0 || model.followingEnd !== nextFollowing
  model.scrollTop = 0
  model.followingEnd = nextFollowing
  model.followSuppressedAtEnd = false
  return Number(changed)
}

function scrollToEnd(model) {
  const next = maximum(model)
  const changed = model.scrollTop !== next || !model.followingEnd
  model.scrollTop = next
  model.followingEnd = true
  model.followSuppressedAtEnd = false
  return Number(changed)
}

function lines(count) {
  return Array.from({ length: count }, (_, index) => `line-${index}`).join('\n')
}

function replayMessage(seed, step, trace) {
  return `seed=0x${seed.toString(16).padStart(8, '0')} step=${step} ` +
    `trace=${trace.slice(-12).join(' -> ')}`
}

test('post-M6 deterministic fullscreen ScrollView state machine stays bounded', () => {
  for (const seed of parseSeeds()) {
    const random = makePrng(seed)
    const terminal = new RecordingTerminal()
    const content = new Text(lines(12), 0, 0)
    const transcript = new ScrollView(content, { follow: 'end', primary: true })
    const root = new VStack([
      { component: transcript, basis: 0, grow: 1, shrink: 1, minSize: 1 },
      { component: new Text('dock', 0, 0), basis: 'auto', grow: 0, shrink: 0, minSize: 1 },
    ], { gap: 1 })
    const tui = new TuiAltScreen(terminal, false)
    const originalRequestRender = tui.requestRender.bind(tui)
    let renderRequests = 0
    tui.requestRender = (force) => {
      renderRequests += 1
      return originalRequestRender(force)
    }
    tui.setLayoutRoot(root)
    tui.start()
    tui.renderNow(true)
    renderRequests = 0

    const model = {
      contentHeight: 12,
      viewportHeight: terminal.rows - 2,
      scrollTop: 0,
      followingEnd: true,
      followSuppressedAtEnd: false,
    }
    updateLayout(model, model.contentHeight, model.viewportHeight)
    const trace = ['init(content=12,rows=8)']

    const check = (step, expectedRequests) => {
      const message = replayMessage(seed, step, trace)
      assert.equal(renderRequests, expectedRequests, `render request count; ${message}`)
      tui.renderNow(true)
      updateLayout(model, model.contentHeight, terminal.rows - 2)
      assert.equal(transcript.viewportHeight, model.viewportHeight, `viewport allocation; ${message}`)
      assert.equal(transcript.scrollTop, model.scrollTop, `scroll upper/lower clamp; ${message}`)
      assert.equal(transcript.isFollowingEnd, model.followingEnd, `follow state; ${message}`)
      assert.equal(tui.viewportTop, model.scrollTop, `fullscreen viewport state; ${message}`)
      assert.equal(tui.isFollowingOutput, model.followingEnd, `fullscreen follow state; ${message}`)
      renderRequests = 0
    }

    // Guaranteed witnesses for both the upper clamp and disable-follow-at-end seams.
    trace.push('scrollTo(high)')
    transcript.scrollTo(model.contentHeight + 50)
    check(-2, scrollTo(model, model.contentHeight + 50, false))
    trace.push('scrollTo(end,disableFollow)')
    transcript.scrollTo(maximum(model), { disableFollow: true })
    check(-1, scrollTo(model, maximum(model), true))

    for (let step = 0; step < 96; step += 1) {
      let expectedRequests = 0
      switch (random(6)) {
        case 0: {
          const count = 1 + random(32)
          trace.push(`content(${count})`)
          content.setText(lines(count))
          model.contentHeight = count
          break
        }
        case 1: {
          const rows = 4 + random(13)
          trace.push(`resize(${rows})`)
          terminal.rows = rows
          break
        }
        case 2: {
          const amount = random(25) - 12
          trace.push(`scrollBy(${amount})`)
          transcript.scrollBy(amount)
          expectedRequests = scrollBy(model, amount)
          break
        }
        case 3: {
          const target = random(model.contentHeight + 25) - 8
          const disableFollow = random(2) === 1
          trace.push(`scrollTo(${target},${disableFollow})`)
          transcript.scrollTo(target, { disableFollow })
          expectedRequests = scrollTo(model, target, disableFollow)
          break
        }
        case 4:
          trace.push('scrollToStart()')
          transcript.scrollToStart()
          expectedRequests = scrollToStart(model)
          break
        default:
          trace.push('scrollToEnd()')
          transcript.scrollToEnd()
          expectedRequests = scrollToEnd(model)
          break
      }
      check(step, expectedRequests)
    }
    tui.stop({ preserveScreen: true })
  }
})
