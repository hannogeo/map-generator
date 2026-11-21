import type { Settings } from './types'

export const defaultSettings: Settings = {
  provider: 'google',
  radius: 500,
  speed: 100,
  strategy: 'random',
  useLandMask: true,
  numOfGenerators: 1,
  mapTheme: 'default',
  coverage: { enabled: true, mode: 'official', blobby: false, colorScheme: 'Default', opacity: 1 },
  rejectUnofficial: true,
  rejectOfficial: false,
  rejectRoadName: false,
  rejectDateless: true,
  rejectNoDescription: true,
  rejectDescription: false,
  checkAllDates: false,
  randomInTimeline: false,
  selectMonths: false,
  fromDate: '2009-01',
  toDate: new Date().toISOString().slice(0, 7),
  fromMonth: '01',
  toMonth: '12',
  fromYear: '2007',
  toYear: String(new Date().getFullYear()),
  oneCountryAtATime: false,
  onlyCheckBlueLines: false,
  disableCheckBlueLine: false,
  findRegions: false,
  regionRadius: 100,
  ignoreBadcam: false,
  findNightCoverage: false,
  onlyOneInTimeframe: false,
  checkLinks: false,
  linksDepth: 2,
  findPhotospheres: false,
  findDrones: false,
  filterByMinutes: { enabled: false, range: [0, 1440] },
  findByTileColor: { enabled: false, zoom: 12, filterType: 'include', operator: 'OR', tileProvider: 'google', tileColors: {} },
  filterByLinksLength: { enabled: false, range: [0, 100] },
  filterByAltitude: { enabled: false, range: [0, 10000] },
  getCurve: false,
  minCurveAngle: 10,
  heading: { adjust: false, reference: 'forward', range: [0, 0], randomInRange: false },
  pitch: { adjust: false, range: [0, 0], randomInRange: false },
  zoom: { adjust: false, range: [0, 0], randomInRange: false },
  searchInDescription: { enabled: false, searchTerms: '', searchMode: 'contains', filterType: 'include' },
  findByAuthor: { enabled: false, author: '', filterType: 'include' },
  findByGeneration: {
    enabled: true,
    google: { 1: false, 2: true, 3: true, 23: true, 4: true, badcam: true },
    apple: { bigcam: true, smallcam: true, backpack: true },
    bing: { 3: true, 4: true, 5: true },
    naver: { 1: true, 3: true, 13: true, 15: true },
    yandex: { 1: true, 2: true, trekker: false },
    mapillary: { 1: true, 2: true },
  },
  apiKeys: { mapycz: '' },
  tag: true,
  panoId: 'enable',
  markers: { noBlueLine: true, newRoad: true, gen4: true, gen2Or3: true, gen1: true },
  notification: { enabled: false, anyLocation: false, anyNoBlueLine: false, onePolygonComplete: false, allPolygonsComplete: false, target: 'desktop', discordWebhook: '' },
}

export function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem('mg:settings')
    if (raw) {
      const saved = JSON.parse(raw)
      // Older builds always showed desktop notifications and had a separate
      // `sendToDiscord` flag. Preserve that behaviour as "both" / "desktop".
      const oldNotification = saved?.notification
      if (oldNotification && !oldNotification.target && typeof oldNotification.sendToDiscord === 'boolean') {
        oldNotification.target = oldNotification.sendToDiscord ? 'both' : 'desktop'
      }
      // The coverage toggle used to be an `official` boolean (true = official,
      // false = all). Keep those saved choices when moving to the three-way mode.
      const oldCoverage = saved?.coverage
      if (oldCoverage && oldCoverage.mode === undefined && typeof oldCoverage.official === 'boolean') {
        oldCoverage.mode = oldCoverage.official ? 'official' : 'all'
      }
      return mergeSettings(defaultSettings, saved)
    }
  } catch {}
  return { ...defaultSettings }
}

/**
 * Recursive merge so settings saved by older builds keep new nested defaults
 * and can never leave a nested object half-populated.
 */
function mergeSettings(base: any, saved: any): any {
  if (Array.isArray(base)) {
    return Array.isArray(saved) ? saved : base
  }
  if (!base || typeof base !== 'object') {
    return saved === undefined || saved === null ? base : saved
  }
  const merged: any = { ...base }
  if (!saved || typeof saved !== 'object' || Array.isArray(saved)) return merged
  for (const key of Object.keys(base)) {
    if (Object.prototype.hasOwnProperty.call(saved, key)) {
      merged[key] = mergeSettings((base as any)[key], saved[key])
    }
  }
  return merged
}

export function saveSettings(s: Settings) {
  localStorage.setItem('mg:settings', JSON.stringify(s))
}