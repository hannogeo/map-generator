export interface LatLng { lat: number; lng: number }

export interface PanoDot {
  panoId: string
  lat: number
  lng: number
}

export interface PanoDotTile {
  x: number
  y: number
  dots: PanoDot[]
}

export interface Marker {
  panoId: string
  lat: number
  lng: number
  heading: number
  pitch: number
  zoom: number
  country?: string
  region?: string
  locality?: string
  road?: string
  imageDate?: string
  source: string
  updateType?: string
  links: string[]
  extra: { tags: string[] }
  markerType?: string
}

export interface Polygon {
  id: string
  name: string
  geometry: any
  nbNeeded: number
  found: Marker[]
  countryCode?: string
}

export type CoverageMode = 'official' | 'unofficial' | 'all'

export interface Settings {
  provider: string
  radius: number
  speed: number
  strategy: string
  useLandMask: boolean
  numOfGenerators: number
  mapTheme: string
  coverage: { enabled: boolean; mode: CoverageMode; blobby: boolean; colorScheme: string; opacity: number }
  rejectUnofficial: boolean
  rejectOfficial: boolean
  rejectRoadName: boolean
  rejectDateless: boolean
  rejectNoDescription: boolean
  rejectDescription: boolean
  checkAllDates: boolean
  randomInTimeline: boolean
  selectMonths: boolean
  fromDate: string
  toDate: string
  fromMonth: string
  toMonth: string
  fromYear: string
  toYear: string
  oneCountryAtATime: boolean
  onlyCheckBlueLines: boolean
  disableCheckBlueLine: boolean
  findRegions: boolean
  regionRadius: number
  ignoreBadcam: boolean
  findNightCoverage: boolean
  onlyOneInTimeframe: boolean
  checkLinks: boolean
  linksDepth: number
  findPhotospheres: boolean
  findDrones: boolean
  filterByMinutes: { enabled: boolean; range: [number, number] }
  findByTileColor: any
  filterByLinksLength: { enabled: boolean; range: [number, number] }
  filterByAltitude: { enabled: boolean; range: [number, number] }
  getCurve: boolean
  minCurveAngle: number
  heading: { adjust: boolean; reference: string; range: [number, number]; randomInRange: boolean }
  pitch: { adjust: boolean; range: [number, number]; randomInRange: boolean }
  zoom: { adjust: boolean; range: [number, number]; randomInRange: boolean }
  searchInDescription: { enabled: boolean; searchTerms: string; searchMode: string; filterType: string }
  findByAuthor: { enabled: boolean; author: string; filterType: string }
  findByGeneration: any
  apiKeys: { mapycz: string }
  tag: boolean
  panoId: string
  markers: { noBlueLine: boolean; newRoad: boolean; gen4: boolean; gen2Or3: boolean; gen1: boolean }
  notification: { enabled: boolean; anyLocation: boolean; anyNoBlueLine: boolean; onePolygonComplete: boolean; allPolygonsComplete: boolean; target: 'desktop' | 'discord' | 'both'; discordWebhook?: string }
}