import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import drawland from './index.js'

test('the postcard builds its graph without the network', () => {
  const g = drawland.graph({ lat: 45, lng: 9 })
  assert.equal(g.output.lat, 45)
  assert.ok(g.draw.length > 0)
})

test("a script's views name styles beside it", () => {
  const dir = mkdtempSync(join(tmpdir(), 'drawland-'))
  writeFileSync(join(dir, 'lines.js'), "output({ scale: 2000 })\ndraw({ id: 'roads', from: lines.of(features.tiles('roads')) })\n")
  const script = "output({ lat: 45, lng: 9 })\nview({ id: 'a', at: [0, 0], size: [50, 40], lat: 41.4, lng: 2.17, scale: '1:4000' }, 'lines')\n"
  assert.equal(drawland.graph({ script, dir }).views[0].id, 'a')
})

test('a script error says where', () => {
  assert.throws(() => drawland.graph({ script: 'areas.nope()' }), /line 1/)
})

test('the postcard is exported', () => {
  assert.match(drawland.POSTCARD, /output\(/)
})
