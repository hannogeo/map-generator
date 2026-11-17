import { readFileSync, writeFileSync } from 'node:fs'

const file = 'version.json'
const data = JSON.parse(readFileSync(file, 'utf8'))
const cmd = process.argv[2]

if (cmd === 'sync') {
  const v = data.version
  // package.json
  const pkg = JSON.parse(readFileSync('package.json', 'utf8'))
  pkg.version = v
  writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n')
  // tauri.conf.json
  const tauri = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
  tauri.version = v
  writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(tauri, null, 2) + '\n')
  // Cargo.toml
  let cargo = readFileSync('src-tauri/Cargo.toml', 'utf8')
  cargo = cargo.replace(/^version = ".*"/m, `version = "${v}"`)
  writeFileSync('src-tauri/Cargo.toml', cargo)
  console.log('synced to', v)
} else if (cmd === 'bump') {
  const parts = data.version.split('.').map(Number)
  parts[2] = (parts[2] || 0) + 1
  data.version = parts.join('.')
  writeFileSync(file, JSON.stringify(data, null, 2) + '\n')
  console.log('bumped to', data.version)
} else {
  console.log('usage: node scripts/version.mjs [sync|bump]')
}