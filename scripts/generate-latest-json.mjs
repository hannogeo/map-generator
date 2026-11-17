import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

const version = JSON.parse(readFileSync('version.json', 'utf8')).version
const tag = 'v' + version
const baseUrl = `https://github.com/hannogeo/map-generator/releases/download/${tag}`

// Find the NSIS setup exe and its signature
const bundleDir = 'src-tauri/target/release/bundle'
function findFile(nameRegex) {
  const results = []
  function walk(dir) {
    for (const entry of readdirSync(dir)) {
      const full = join(dir, entry)
      if (statSync(full).isDirectory()) walk(full)
      else if (nameRegex.test(entry)) results.push(full)
    }
  }
  walk(bundleDir)
  return results[0]
}

const exePath = findFile(/-setup\.exe$/)
const sigPath = findFile(/-setup\.exe\.sig$/)
if (!exePath || !sigPath) {
  console.error('Could not find setup exe or signature')
  process.exit(1)
}

const exeName = exePath.split(/[\\/]/).pop()
const publishedName = exeName.replace(/ /g, '.')
const signature = readFileSync(sigPath, 'utf8').trim()

const latest = {
  version,
  notes: '',
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url: `${baseUrl}/${publishedName}`,
    },
  },
}

writeFileSync('latest.json', JSON.stringify(latest, null, 2) + '\n')
console.log('Generated latest.json for', version)
console.log('exe:', exeName)
