// Builds the compact land mask used by the generation engine to skip ocean
// coordinates. Source data: Natural Earth 1:50m land polygons (public domain).
//
// Usage: node scripts/build-land-mask.mjs
//
// Output: src-tauri/assets/land_mask.bin
//
// Format (little endian):
//   magic  "MGL1" (4 bytes)
//   cols   u16   number of longitude cells
//   rows   u16   number of latitude cells
//   step   f32   cell size in degrees
//   rows × { count u16, count × { start u16, end u16 } }  (inclusive columns)
//
// Rows run south to north, columns west to east.

import fs from "node:fs"
import path from "node:path"

const STEP = 0.05
const ROOT = path.resolve(import.meta.dirname, "..")
const CACHE = path.join(ROOT, "node_modules", ".cache", "ne_50m_land.geojson")
const SOURCE = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_50m_land.geojson"
const OUTPUT = path.join(ROOT, "src-tauri", "assets", "land_mask.bin")

async function loadLand() {
  if (fs.existsSync(CACHE)) return JSON.parse(fs.readFileSync(CACHE, "utf8"))
  const response = await fetch(SOURCE)
  if (!response.ok) throw new Error(`Failed to download land data: ${response.status}`)
  const text = await response.text()
  fs.mkdirSync(path.dirname(CACHE), { recursive: true })
  fs.writeFileSync(CACHE, text)
  return JSON.parse(text)
}

function ringsOf(geometry, out = []) {
  if (!geometry) return out
  if (geometry.type === "Polygon") {
    for (const ring of geometry.coordinates) out.push(ring)
  } else if (geometry.type === "MultiPolygon") {
    for (const polygon of geometry.coordinates) for (const ring of polygon) out.push(ring)
  } else if (geometry.type === "GeometryCollection") {
    for (const child of geometry.geometries) ringsOf(child, out)
  }
  return out
}

function rasterize(features) {
  const cols = Math.round(360 / STEP)
  const rows = Math.round(180 / STEP)
  const rowSpans = []

  const edges = []
  for (const feature of features) {
    for (const ring of ringsOf(feature.geometry)) {
      for (let i = 0; i + 1 < ring.length; i++) {
        let [x1, y1] = ring[i]
        let [x2, y2] = ring[i + 1]
        if (y1 === y2) continue
        if (y1 > y2) {
          ;[x1, x2] = [x2, x1]
          ;[y1, y2] = [y2, y1]
        }
        edges.push({ y1, y2, x1, inv: (x2 - x1) / (y2 - y1) })
      }
    }
  }
  edges.sort((a, b) => a.y1 - b.y1)

  let edgeIndex = 0
  const active = []
  for (let row = 0; row < rows; row++) {
    const lat = -90 + (row + 0.5) * STEP
    while (edgeIndex < edges.length && edges[edgeIndex].y1 <= lat) {
      if (lat < edges[edgeIndex].y2) active.push(edges[edgeIndex])
      edgeIndex++
    }
    for (let i = active.length - 1; i >= 0; i--) if (lat >= active[i].y2) active.splice(i, 1)

    const crossings = active.map((edge) => edge.x1 + (lat - edge.y1) * edge.inv).sort((a, b) => a - b)
    const spans = []
    for (let i = 0; i + 1 < crossings.length; i += 2) {
      const start = Math.max(0, Math.ceil((crossings[i] + 180) / STEP - 0.5))
      const end = Math.min(cols - 1, Math.floor((crossings[i + 1] + 180) / STEP - 0.5))
      if (end >= start) spans.push([start, end])
    }
    rowSpans.push(spans)
  }
  return { cols, rows, rowSpans }
}

const land = await loadLand()
const { cols, rows, rowSpans } = rasterize(land.features)

const parts = []
const header = Buffer.alloc(12)
header.write("MGL1", 0, "ascii")
header.writeUInt16LE(cols, 4)
header.writeUInt16LE(rows, 6)
header.writeFloatLE(STEP, 8)
parts.push(header)

let spanCount = 0
for (const spans of rowSpans) {
  const count = Buffer.alloc(2)
  count.writeUInt16LE(spans.length, 0)
  parts.push(count)
  const data = Buffer.alloc(spans.length * 4)
  spans.forEach(([start, end], index) => {
    data.writeUInt16LE(start, index * 4)
    data.writeUInt16LE(end, index * 4 + 2)
  })
  parts.push(data)
  spanCount += spans.length
}

const output = Buffer.concat(parts)
fs.mkdirSync(path.dirname(OUTPUT), { recursive: true })
fs.writeFileSync(OUTPUT, output)
console.log(`wrote ${OUTPUT} (${(output.length / 1024).toFixed(1)} KiB, ${cols}x${rows} cells, ${spanCount} spans)`)
