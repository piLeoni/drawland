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

test('a graph built in Node is the script\'s', () => {
  const js = `output({ lat: 37.802, lng: -122.263, width: "6in", height: "4in", scale: "1:8000" })
const water = areas.polygons(features.tiles("water"))
draw({ id: "water", from: fills.fill({ in: water, pattern: "waves", spacing: "0.8mm" }) })
`
  const g = new drawland.Graph()
  g.output({ lat: 37.802, lng: -122.263, width: '6in', height: '4in', scale: '1:8000' })
  const water = g.areas.polygons(g.features.tiles('water'))
  g.draw({ id: 'water', from: g.fills.fill({ in: water, pattern: 'waves', spacing: '0.8mm' }) })
  const strip = (v) => (Array.isArray(v) ? v.map(strip) : v && typeof v === 'object' ? Object.fromEntries(Object.entries(v).filter(([k]) => k !== 'at').map(([k, x]) => [k, strip(x)])) : v)
  assert.deepEqual(strip(g.finish()), strip(drawland.graph({ script: js })))
})

test('Node functions for views and remaps, and module parts', () => {
  const g = new drawland.Graph()
  g.output({ lat: 45, lng: 9, scale: '1:20000', warp: { remap: ([x, y]) => [x, y * 0.5], grid: 4 } })
  assert.ok(g.page.width > 0)
  assert.equal(g.view({ id: 'inset', at: [0, 0], size: [40, 30], lat: 41.4, lng: 2.17, scale: '1:4000' }, () => {
    g.draw({ id: 'roads', from: g.lines.of(g.features.tiles('roads')) })
  }), 'inset')
  const labels = g.modules.labels({ from: g.features.tiles('roads'), keys: 'name', rules: [{ tags: { highway: 'primary' }, id: 'big' }] })
  g.draw({ id: 'labels', from: labels.lines })
  g.compose.mask({ from: labels.big.area, occludes: ['labels'] })
  const d = g.finish()
  assert.equal(d.views.length, 1)
  assert.ok('mesh' in d.output.warp[0])
})

test('Node gets the same mistakes', () => {
  const g = new drawland.Graph()
  assert.throws(() => g.areas.union(1), /areas.union expects an area/)
})

test('TypeScript', () => {
  const script = "const s: string = '0.5mm'\noutput({ lat: 45, lng: 9 })\ndraw({ id: 'r', from: lines.of(features.tiles('roads')) as Lines })\n"
  assert.ok(drawland.graph({ script, typescript: true }).draw.length)
})
