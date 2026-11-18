<template>
  <div class="app-shell">
    <div id="map"></div>

    <div class="columns">
      <div class="column left">
        <div class="logo">
          <span>Map Generator</span>
        </div>

        <details class="section" open>
          <summary>General</summary>
          <div class="body">
            <div class="field">
              <span>Map theme</span>
              <select v-model="settings.mapTheme" @change="setBase(settings.mapTheme)">
                <option value="default">Default</option>
                <option value="dark">Dark</option>
                <option value="night">Night</option>
                <option value="classic">Classic</option>
                <option value="retro">Retro</option>
                <option value="aubergine">Aubergine</option>
                <option value="satellite">Satellite</option>
                <option value="terrain">Terrain</option>
                <option value="carto">Carto</option>
                <option value="osm">OSM</option>
              </select>
            </div>
            <div class="field">
              <span>Street View overlay</span>
              <select v-model="settings.coverage.enabled" @change="applyCoverage">
                <option :value="true">On</option>
                <option :value="false">Off</option>
              </select>
            </div>
            <div class="nested" v-if="settings.coverage.enabled">
              <div class="field">
                <span>Coverage</span>
                <select v-model="settings.coverage.official" @change="applyCoverage">
                  <option :value="true">Official</option>
                  <option :value="false">All</option>
                </select>
              </div>
              <div class="field">
                <span>Colour scheme</span>
                <select v-model="settings.coverage.colorScheme" @change="applyCoverage">
                  <option v-for="(_, name) in COLOR_SCHEMES" :key="name" :value="name">
                    {{ String(name).replaceAll('_', ' ') }}
                  </option>
                </select>
              </div>
              <label class="check">
                <input type="checkbox" v-model="settings.coverage.blobby" @change="applyCoverage" />
                Blobby layer
              </label>
              <div class="field">
                <span>Opacity</span>
                <div class="pair">
                  <input type="range" min="0" max="1" step="0.05" v-model.number="settings.coverage.opacity" />
                  <span class="value">{{ Math.round(settings.coverage.opacity * 100) }}%</span>
                </div>
              </div>
            </div>
            <div class="field">
              <span>Notifications</span>
              <select v-model="settings.notification.enabled" @change="onNotificationToggle">
                <option :value="true">On</option>
                <option :value="false">Off</option>
              </select>
            </div>
            <div class="nested" v-if="settings.notification.enabled">
              <label class="check"><input type="checkbox" v-model="settings.notification.anyLocation" /> First location in polygon</label>
              <label class="check"><input type="checkbox" v-model="settings.notification.anyNoBlueLine" /> First no-blue-line location</label>
              <label class="check"><input type="checkbox" v-model="settings.notification.onePolygonComplete" /> One polygon complete</label>
              <label class="check"><input type="checkbox" v-model="settings.notification.allPolygonsComplete" /> All polygons complete</label>
              <label class="check" v-if="isGoogle">
                <input type="checkbox" v-model="settings.notification.sendToDiscord" /> Send to Discord
              </label>
              <div class="field" v-if="isGoogle && settings.notification.sendToDiscord">
                <span>Webhook</span>
                <div class="pair">
                  <input :type="showWebhook ? 'text' : 'password'" v-model="settings.notification.discordWebhook" placeholder="Discord webhook URL" />
                  <button class="icon" type="button" @click="showWebhook = !showWebhook" :title="showWebhook ? 'Hide' : 'Show'">
                    {{ showWebhook ? '🙈' : '👁' }}
                  </button>
                </div>
              </div>
            </div>
            <div class="field" v-if="settings.provider === 'mapycz'">
              <span>Mapy.cz key</span>
              <input type="password" v-model="settings.apiKeys.mapycz" placeholder="API key" />
            </div>
          </div>
        </details>

        <details class="section" open>
          <summary>Countries / territories <span class="badge">{{ polygons.length }}</span></summary>
          <div class="body">
            <div class="search-wrap">
              <input
                type="text"
                v-model="countryQuery"
                placeholder="Search country or territory…"
                @focus="searchOpen = true"
                @input="searchOpen = true"
                @keydown.esc="countryQuery = ''"
              />
              <div class="search-results" v-if="countryQuery.trim() && searchOpen">
                <button v-for="c in countryMatches" :key="c.index" type="button" @click="pickCountry(c.index)">
                  <span class="country-name">{{ c.name }}</span>
                  <span class="country-parent" v-if="c.country && c.country !== c.name">{{ c.country }}</span>
                </button>
                <div class="hint" v-if="!countryMatches.length">No matches</div>
              </div>
            </div>
            <div class="actions">
              <button @mousedown.prevent @click="toggleDraw">{{ drawing ? 'Finish drawing' : 'Draw polygon' }}</button>
              <button @mousedown.prevent @click="importGeoJson">Import GeoJSON</button>
              <button @mousedown.prevent @click="clearPolygons">Deselect all</button>
            </div>
            <div class="hint" v-if="!polygons.length">Click countries on the map, search above, or draw your own polygon.</div>
            <div class="polygon-list" v-else>
              <div class="polygon-row" v-for="p in polygons" :key="p.id">
                <span class="polygon-name" :title="p.name" @click="pickCountry(Number(p.id.replace('country-','')))" >{{ p.name }}</span>
                <span class="polygon-count">
                  {{ p.found.length }} /
                  <input type="number" min="0" :value="p.nbNeeded" @change="setNeeded(p, $event)" />
                </span>
                <div class="row-actions">
                  <button class="icon" title="Clear locations" :disabled="!p.found.length" @click="clearPolygonLocations(p)">Clear</button>
                  <button class="icon danger" title="Remove country" @click="removePolygon(p.id)">×</button>
                </div>
              </div>
            </div>
            <div class="actions" v-if="markers.length">
              <button class="danger clear-all" @click="askClearAllLocations">Clear all locations</button>
            </div>
          </div>
        </details>

        <details class="section" open>
          <summary>Export all <span class="badge">{{ markers.length }}</span></summary>
          <div class="body">
            <div class="actions">
              <button :disabled="!markers.length" @click="copyMarkers(markers)">Copy JSON</button>
              <button :disabled="!markers.length" :title="markers.length ? exportFileName + '.json' : ''" @click="exportJson(markers)">JSON</button>
              <button :disabled="!markers.length" :title="markers.length ? exportFileName + '.csv' : ''" @click="exportCsv(markers)">CSV</button>
            </div>
          </div>
        </details>

        <div class="export-notice" v-if="exportNotice" aria-live="polite">{{ exportNotice }}</div>
      </div>

      <div class="column right">
        <details class="section" open>
          <summary>Generator settings</summary>
          <div class="body">
            <div class="field">
              <span>Provider</span>
              <select v-model="settings.provider" @change="onProviderChange">
                <option v-for="p in providers" :key="p" :value="p">{{ providerLabel(p) }}</option>
              </select>
            </div>
            <div class="field">
              <span>Pano ID</span>
              <select v-model="settings.panoId">
                <option value="enable">Enable</option>
                <option value="disable">Disable</option>
                <option value="prefix">Prefix</option>
              </select>
            </div>
            <div class="field">
              <span>Strategy</span>
              <select v-model="settings.strategy">
                <option value="random">Random</option>
                <option value="grid">Grid</option>
              </select>
            </div>
            <div class="field">
              <span>Generators</span>
              <input type="number" min="1" max="10" v-model.number="settings.numOfGenerators" />
            </div>
            <div class="field">
              <span>Speed (attempts)</span>
              <input type="number" min="1" max="1000" v-model.number="settings.speed" />
            </div>
            <div class="field" v-if="radiusVisible">
              <span>Radius (m)</span>
              <input type="number" min="1" v-model.number="settings.radius" />
            </div>
            <label class="check"><input type="checkbox" v-model="settings.oneCountryAtATime" /> One country at a time</label>
            <label class="check"><input type="checkbox" v-model="settings.onlyCheckBlueLines" /> Only check areas with blue lines</label>
            <label class="check"><input type="checkbox" v-model="settings.disableCheckBlueLine" /> Disable no-blue-line check</label>
            <label class="check" v-if="!settings.rejectOfficial"><input type="checkbox" v-model="settings.findRegions" /> Minimum distance between locations</label>
            <div class="field" v-if="settings.findRegions">
              <span>Distance (km)</span>
              <input type="number" min="1" v-model.number="settings.regionRadius" />
            </div>
            <label class="check"><input type="checkbox" v-model="settings.tag" /> Auto-tag locations</label>
          </div>
        </details>

        <details class="section" open>
          <summary>Coverage settings</summary>
          <div class="body">
            <label class="check"><input type="checkbox" v-model="settings.rejectUnofficial" /> Reject unofficial</label>
            <label class="check" v-if="!settings.rejectUnofficial && !settings.rejectOfficial">
              <input type="checkbox" v-model="settings.findByAuthor.enabled" /> Find by author
            </label>
            <div class="nested" v-if="settings.findByAuthor.enabled && !settings.rejectUnofficial && !settings.rejectOfficial">
              <div class="field">
                <span>Include / exclude</span>
                <select v-model="settings.findByAuthor.filterType">
                  <option value="include">include</option>
                  <option value="exclude">exclude</option>
                </select>
              </div>
              <input type="text" v-model.trim="settings.findByAuthor.author" placeholder="Author name" />
            </div>
            <label class="check"><input type="checkbox" v-model="settings.rejectOfficial" /> Find unofficial coverage</label>
            <label class="check" v-if="settings.rejectOfficial"><input type="checkbox" v-model="settings.findPhotospheres" /> Find photospheres only</label>
            <label class="check" v-if="settings.rejectOfficial"><input type="checkbox" v-model="settings.findDrones" /> Find drone photospheres only</label>
            <template v-if="settings.rejectUnofficial && !settings.rejectOfficial">
              <label class="check"><input type="checkbox" v-model="settings.rejectDateless" /> Reject locations without date</label>
              <label class="check" v-if="!settings.rejectDescription"><input type="checkbox" v-model="settings.rejectNoDescription" /> Reject locations without description</label>
              <label class="check" v-if="!settings.rejectOfficial"><input type="checkbox" v-model="settings.rejectRoadName" /> Reject locations with road name</label>
              <label class="check" v-if="isGoogle"><input type="checkbox" v-model="settings.rejectDescription" /> Find trekker coverage (Gen 3 only)</label>
              <label class="check" v-if="settings.provider === 'tencent'"><input type="checkbox" v-model="settings.findNightCoverage" /> Find night coverage</label>
              <label class="check"><input type="checkbox" v-model="settings.onlyOneInTimeframe" /> Only one panorama per location</label>
              <label class="check"><input type="checkbox" v-model="settings.checkLinks" /> Check linked panos</label>
              <div class="field" v-if="settings.checkLinks">
                <span>Link depth</span>
                <div class="pair">
                  <input type="range" min="1" max="10" step="1" v-model.number="settings.linksDepth" />
                  <span class="value">{{ settings.linksDepth }}</span>
                </div>
              </div>
              <label class="check" v-if="isGoogle"><input type="checkbox" v-model="settings.ignoreBadcam" /> Ignore BadCam</label>
              <template v-if="generationOptions.length">
                <label class="check"><input type="checkbox" v-model="generationEnabled" /> Find by generation</label>
                <div class="nested" v-if="generationEnabled">
                  <label class="check" v-for="item in generationOptions" :key="item.key">
                    <input type="checkbox" :checked="isGenerationChecked(item.key)" @change="setGenerationChecked(item.key, $event)" />
                    {{ item.label }}
                  </label>
                </div>
              </template>
            </template>

            <div class="field" v-if="!settings.selectMonths">
              <span>From</span>
              <input type="month" v-model="settings.fromDate" min="2007-01" />
            </div>
            <div class="field" v-if="!settings.selectMonths">
              <span>To</span>
              <input type="month" v-model="settings.toDate" min="2007-01" />
            </div>
            <label class="check" v-if="!settings.rejectOfficial"><input type="checkbox" v-model="settings.selectMonths" /> Filter by months</label>
            <div class="nested" v-if="settings.selectMonths">
              <div class="field">
                <span>From month</span>
                <select v-model="settings.fromMonth">
                  <option v-for="m in months" :key="m.value" :value="m.value">{{ m.label }}</option>
                </select>
              </div>
              <div class="field">
                <span>To month</span>
                <select v-model="settings.toMonth">
                  <option v-for="m in months" :key="m.value" :value="m.value">{{ m.label }}</option>
                </select>
              </div>
              <div class="field">
                <span>From year</span>
                <input type="number" min="2007" v-model.number="settings.fromYear" />
              </div>
              <div class="field">
                <span>To year</span>
                <input type="number" min="2007" v-model.number="settings.toYear" />
              </div>
            </div>
            <div v-if="!isGoogle">
              <label class="check"><input type="checkbox" v-model="settings.filterByMinutes.enabled" /> Filter by minutes</label>
              <div class="field" v-if="settings.filterByMinutes.enabled">
                <span>Range</span>
                <div class="pair">
                  <input type="number" min="0" max="1439" v-model.number="settings.filterByMinutes.range[0]" />
                  <input type="number" min="0" max="1439" v-model.number="settings.filterByMinutes.range[1]" />
                </div>
              </div>
            </div>
            <label class="check"><input type="checkbox" v-model="settings.checkAllDates" /> Check all dates</label>
            <label class="check" v-if="settings.rejectUnofficial && !settings.rejectOfficial">
              <input type="checkbox" v-model="settings.randomInTimeline" /> Choose random date in range
            </label>
          </div>
        </details>

        <details class="section">
          <summary>Map making settings</summary>
          <div class="body">
            <label class="check"><input type="checkbox" v-model="settings.searchInDescription.enabled" /> Search in panorama description</label>
            <div class="nested" v-if="settings.searchInDescription.enabled">
              <div class="field">
                <span>Include / exclude</span>
                <select v-model="settings.searchInDescription.filterType">
                  <option value="include">include</option>
                  <option value="exclude">exclude</option>
                </select>
              </div>
              <input type="text" v-model.trim="settings.searchInDescription.searchTerms" placeholder="Terms, comma separated" />
              <div class="field">
                <span>Search mode</span>
                <select v-model="settings.searchInDescription.searchMode">
                  <option value="contains">contains</option>
                  <option value="fullword">fullword</option>
                  <option value="sectionmatch">sectionmatch</option>
                  <option value="startswith">startswith</option>
                  <option value="endswith">endswith</option>
                </select>
              </div>
            </div>
            <label class="check"><input type="checkbox" v-model="settings.filterByLinksLength.enabled" /> Filter by links length</label>
            <div class="field" v-if="settings.filterByLinksLength.enabled">
              <span>Range</span>
              <div class="pair">
                <input type="number" min="0" max="10" v-model.number="settings.filterByLinksLength.range[0]" />
                <input type="number" min="0" max="10" v-model.number="settings.filterByLinksLength.range[1]" />
              </div>
            </div>
            <label class="check" v-if="altitudeProviders.includes(settings.provider)">
              <input type="checkbox" v-model="settings.filterByAltitude.enabled" /> Filter by altitude
            </label>
            <div class="field" v-if="settings.filterByAltitude.enabled">
              <span>Meters</span>
              <div class="pair">
                <input type="number" min="-200" max="8848" v-model.number="settings.filterByAltitude.range[0]" />
                <input type="number" min="-200" max="8848" v-model.number="settings.filterByAltitude.range[1]" />
              </div>
            </div>
            <label class="check"><input type="checkbox" v-model="settings.getCurve" /> Find curve locations</label>
            <div class="field" v-if="settings.getCurve">
              <span>Min curve angle</span>
              <div class="pair">
                <input type="range" min="5" max="90" step="1" v-model.number="settings.minCurveAngle" />
                <span class="value">{{ settings.minCurveAngle }}°</span>
              </div>
            </div>

            <label class="check"><input type="checkbox" v-model="settings.heading.adjust" /> Set heading</label>
            <div class="nested" v-if="settings.heading.adjust">
              <label class="check"><input type="radio" value="link" v-model="settings.heading.reference" /> Along road</label>
              <label class="check"><input type="radio" value="forward" v-model="settings.heading.reference" /> To front of car</label>
              <label class="check"><input type="radio" value="backward" v-model="settings.heading.reference" /> To back of car</label>
              <div class="field">
                <span>Deviation</span>
                <div class="pair">
                  <input type="number" min="-180" max="180" v-model.number="settings.heading.range[0]" />
                  <input type="number" min="-180" max="180" v-model.number="settings.heading.range[1]" />
                </div>
              </div>
              <label class="check"><input type="checkbox" v-model="settings.heading.randomInRange" /> Random in range</label>
            </div>
            <label class="check"><input type="checkbox" v-model="settings.pitch.adjust" /> Set pitch</label>
            <div class="nested" v-if="settings.pitch.adjust">
              <div class="field">
                <span>Range</span>
                <div class="pair">
                  <input type="number" min="-90" max="90" v-model.number="settings.pitch.range[0]" />
                  <input type="number" min="-90" max="90" v-model.number="settings.pitch.range[1]" />
                </div>
              </div>
              <label class="check"><input type="checkbox" v-model="settings.pitch.randomInRange" /> Random in range</label>
            </div>
            <label class="check"><input type="checkbox" v-model="settings.zoom.adjust" /> Set zoom</label>
            <div class="nested" v-if="settings.zoom.adjust">
              <div class="field">
                <span>Range</span>
                <div class="pair">
                  <input type="number" min="0" max="4" step="1" v-model.number="settings.zoom.range[0]" />
                  <input type="number" min="0" max="4" step="1" v-model.number="settings.zoom.range[1]" />
                </div>
              </div>
              <label class="check"><input type="checkbox" v-model="settings.zoom.randomInRange" /> Random in range</label>
            </div>
          </div>
        </details>

        <details class="section" open>
          <summary>Markers</summary>
          <div class="body">
            <label class="check" v-if="isGoogle"><input type="checkbox" v-model="settings.markers.noBlueLine" @change="refreshMarkerVisibility" /><span class="dot" style="background:#e412d2"></span> No blue line</label>
            <label class="check"><input type="checkbox" v-model="settings.markers.newRoad" @change="refreshMarkerVisibility" /><span class="dot" style="background:#ca283f"></span> New road</label>
            <label class="check"><input type="checkbox" v-model="settings.markers.gen4" @change="refreshMarkerVisibility" /><span class="dot" style="background:#2880ca"></span> {{ isGoogle ? 'Gen 4 update' : 'Update' }}</label>
            <label class="check" v-if="isGoogle"><input type="checkbox" v-model="settings.markers.gen2Or3" @change="refreshMarkerVisibility" /><span class="dot" style="background:#9a28ca"></span> Gen 2 or 3 update</label>
            <label class="check" v-if="isGoogle"><input type="checkbox" v-model="settings.markers.gen1" @change="refreshMarkerVisibility" /><span class="dot" style="background:#24ac20"></span> Gen 1 update</label>
            <label class="check"><input type="checkbox" v-model="markersVisible" @change="toggleMarkerLayer" /> Show pins on map</label>
          </div>
        </details>

        <div class="run-row">
          <button class="primary big" :disabled="!polygons.length" @click="toggleRun">
            {{ running ? (stopping ? 'Stopping…' : 'Stop generation') : 'Start generation' }}
          </button>
          <button class="big danger" @click="resetGeneration">Reset</button>
        </div>
      </div>
    </div>

    <div class="statusbar" v-if="polygons.length">
      <span>{{ markers.length }} / {{ totalGoal }} locations</span>
    </div>
    <div class="zoom-indicator" :class="{ down: !polygons.length }">Zoom {{ currentZoom }}</div>

    <div v-if="confirmOpen" class="modal-backdrop" @click="cancelReset">
      <div class="modal" @click.stop>
        <div class="modal-title">Reset generation?</div>
        <div class="modal-body">This clears all found locations and removes the selected countries. Settings are kept.</div>
        <div class="modal-actions">
          <button @click="cancelReset">Cancel</button>
          <button class="danger" @click="confirmReset">Reset</button>
        </div>
      </div>
    </div>

    <div v-if="clearAllOpen" class="modal-backdrop" @click="clearAllOpen = false">
      <div class="modal" @click.stop>
        <div class="modal-title">Clear all locations?</div>
        <div class="modal-body">This removes {{ locationCount(markers.length) }} from the map. Selected countries and their goals are kept.</div>
        <div class="modal-actions">
          <button @click="clearAllOpen = false">Cancel</button>
          <button class="danger" @click="confirmClearAllLocations">Clear</button>
        </div>
      </div>
    </div>

    <div v-if="updateAvailable" class="update-banner">
      <span>Update {{ updateVersion }} available</span>
      <div class="update-progress" v-if="updating">
        <div class="update-progress-bar" :style="{ width: updateProgress + '%' }"></div>
      </div>
      <button @click="installUpdate" :disabled="updating">{{ updating ? 'Downloading… ' + updateProgress + '%' : 'Download & restart' }}</button>
      <span v-if="updateError" class="update-error">{{ updateError }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, toRaw, watch } from 'vue'
import L from 'leaflet'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import {
  isPermissionGranted as notificationGranted,
  requestPermission as requestNotificationPermission,
} from '@tauri-apps/plugin-notification'
import type { Marker, Polygon, Settings } from './types'
import { loadSettings, saveSettings } from './settings'
import {
  COLOR_SCHEMES,
  GOOGLE_LABELS,
  GOOGLE_LABELS_SATELLITE,
  GOOGLE_ROADMAP,
  GOOGLE_SATELLITE,
  GOOGLE_TERRAIN,
  streetViewUrl,
} from './constants'
import {
  GOOGLE_LABELS_AUBERGINE,
  GOOGLE_LABELS_CLASSIC,
  GOOGLE_LABELS_DARK,
  GOOGLE_LABELS_NIGHT,
  GOOGLE_LABELS_RETRO,
  GOOGLE_ROADMAP_AUBERGINE,
  GOOGLE_ROADMAP_CLASSIC,
  GOOGLE_ROADMAP_DARK,
  GOOGLE_ROADMAP_NIGHT,
  GOOGLE_ROADMAP_RETRO,
} from './google-themes'

type MarkerRecord = Marker & { polygonId?: string }

const providers = [
  'google', 'googleZoom', 'apple', 'yandex', 'bing', 'naver', 'mapillary',
  'baidu', 'tencent', 'kakao', 'mapycz', 'openmap', 'asig', 'ja', 'vegbilder',
]
const providerNames: Record<string, string> = {
  google: 'Google', googleZoom: 'Google (zoom)', apple: 'Apple', yandex: 'Yandex',
  bing: 'Bing', naver: 'Naver', mapillary: 'Mapillary', baidu: 'Baidu',
  tencent: 'Tencent', kakao: 'Kakao', mapycz: 'Mapy.cz', openmap: 'OpenMap',
  asig: 'ASIG', ja: 'Já', vegbilder: 'Vegbilder',
}
const altitudeProviders = ['google', 'googleZoom', 'bing', 'naver', 'mapillary', 'baidu', 'mapycz', 'asig', 'tencent']
const months = [
  { value: '01', label: 'January' }, { value: '02', label: 'February' },
  { value: '03', label: 'March' }, { value: '04', label: 'April' },
  { value: '05', label: 'May' }, { value: '06', label: 'June' },
  { value: '07', label: 'July' }, { value: '08', label: 'August' },
  { value: '09', label: 'September' }, { value: '10', label: 'October' },
  { value: '11', label: 'November' }, { value: '12', label: 'December' },
]

const settings = ref<Settings>(loadSettings())
const polygons = ref<Polygon[]>([])
const markers = ref<MarkerRecord[]>([])
const running = ref(false)
const stopping = ref(false)
const status = ref('Ready')
const drawing = ref(false)
const drawPoints = ref<L.LatLng[]>([])
const drawLine = ref<L.Polyline | null>(null)
const drawMarkers = ref<L.CircleMarker[]>([])
const currentZoom = ref(3)
const countryQuery = ref('')
const countryFeatures = ref<any[]>([])
const countryLayers = new Map<number, L.GeoJSON>()
const searchOpen = ref(true)
const updateAvailable = ref(false)
const updateVersion = ref('')
const showWebhook = ref(false)
const markersVisible = ref(true)
const totalGoal = computed(() => polygons.value.reduce((sum, polygon) => sum + (Number(polygon.nbNeeded) || 0), 0))
let updateHandle: any = null
const updating = ref(false)
const updateProgress = ref(-1)
const updateError = ref('')

const isGoogle = computed(() => settings.value.provider.includes('google'))
const radiusVisible = computed(() => !['apple', 'naver', 'yandex', 'googleZoom'].includes(settings.value.provider))

let map: L.Map
let markerLayer: L.LayerGroup
let customLayer: L.LayerGroup
// Markers and drawn overlays use SVG renderers: a Canvas renderer fills the
// whole pane and swallows every map click, which made country selection stop
// working as soon as one marker existed.
let markerRenderer: L.SVG
let customRenderer: L.SVG
let countryLayer: L.GeoJSON | null = null
let activeBase: L.Layer | null = null
let activeLabels: L.Layer | null = null
let coverageLayer: L.TileLayer | null = null
// base cache no longer used
const markerLayers: Array<{ marker: MarkerRecord; layer: L.CircleMarker }> = []
const unlisteners: Array<() => void> = []
let unlistenClose: (() => void) | undefined

const generationDefinitions: Record<string, Array<{ key: string; label: string }>> = {
  google: [
    { key: '1', label: 'Gen 1' },
    { key: '2', label: 'Gen 2' },
    { key: '3', label: 'Gen 3' },
    { key: '23', label: 'Gen 2 & 3' },
    { key: '4', label: 'Gen 4' },
    { key: 'badcam', label: 'BadCam' },
  ],
  apple: [
    { key: 'bigcam', label: 'Big camera' },
    { key: 'smallcam', label: 'Small camera' },
    { key: 'backpack', label: 'Backpack' },
  ],
  bing: [
    { key: '3', label: 'TomTom' },
    { key: '4', label: 'Bing' },
    { key: '5', label: 'Bing 5' },
  ],
  naver: [
    { key: '1', label: 'Drone' },
    { key: '3', label: 'Gen 2' },
    { key: '4', label: 'Gen 1' },
    { key: '13', label: 'Trekker' },
    { key: '15', label: 'Gen 3 (3D)' },
  ],
  yandex: [
    { key: '1', label: 'Gen 1' },
    { key: '2', label: 'Gen 2' },
    { key: 'trekker', label: 'Trekker' },
  ],
  mapillary: [
    { key: '1', label: 'Panorama' },
    { key: '2', label: 'Photo' },
  ],
}

const generationProviderKey = computed(() =>
  settings.value.provider === 'googleZoom' ? 'google' : settings.value.provider,
)
const generationOptions = computed(() => generationDefinitions[generationProviderKey.value] || [])
const generationEnabled = computed({
  get: () => !!settings.value.findByGeneration?.enabled,
  set: (value: boolean) => {
    if (!settings.value.findByGeneration) settings.value.findByGeneration = {}
    settings.value.findByGeneration.enabled = value
  },
})

const countryMatches = computed(() => {
  const query = countryQuery.value.trim().toLowerCase()
  if (!query) return []
  return countryFeatures.value
    .map((feature, index) => ({
      index,
      code: String(feature.properties?.code || ''),
      name: String(feature.properties?.name || ''),
      country: String(feature.properties?.country || ''),
    }))
    .filter(country => country.name.toLowerCase().includes(query) || country.country.toLowerCase().includes(query))
    .sort((a, b) => a.name.localeCompare(b.name))
    .slice(0, 14)
})

watch(settings, value => saveSettings(value), { deep: true })
watch(() => polygons.value.map(p => p.id).join(','), refreshCountryStyles)
watch(() => settings.value.coverage.opacity, value => { if (coverageLayer) coverageLayer.setOpacity(value) })

onMounted(async () => {
  map = L.map('map', {
    minZoom: 2,
    maxZoom: 20,
    zoomControl: false,
    attributionControl: false,
    preferCanvas: true,
    worldCopyJump: true,
  }).setView([28, 12], 3)

  L.control.zoom({ position: 'bottomright' }).addTo(map)
  map.createPane('countriesPane')
  map.getPane('countriesPane')!.style.zIndex = '350'
  map.createPane('customPane')
  map.getPane('customPane')!.style.zIndex = '360'
  map.createPane('markersPane')
  map.getPane('markersPane')!.style.zIndex = '650'

  markerRenderer = L.svg({ pane: 'markersPane' })
  customRenderer = L.svg({ pane: 'customPane' })
  markerLayer = L.layerGroup().addTo(map)
  customLayer = L.layerGroup().addTo(map)

  setBase(settings.value.mapTheme || 'default')
  applyCoverage()
  map.on('zoomend', () => { currentZoom.value = map.getZoom() })
  map.on('click', onMapClick)
  map.on('dblclick', finishDraw)

  await loadCountries()
  await loadSavedPolygons()

  unlisteners.push(await listen('generation:location', (event: any) => {
    const payload = event.payload
    const polygon = polygons.value.find(p => p.id === payload.polygonId)
    // Late events from a stopped or reset run are dropped: the polygon is gone.
    if (!polygon) return
    const marker = payload.location as MarkerRecord
    if (markers.value.some(m => m.panoId === marker.panoId && m.source === marker.source)) return
    marker.polygonId = payload.polygonId
    markers.value.push(marker)
    addMarkerToMap(marker)
    polygon.found.push(marker)
    setProgressStatus(`${polygon.name}: ${polygon.found.length}/${polygon.nbNeeded}`)
    persistPolygons()
  }))
  unlisteners.push(await listen('generation:progress', (event: any) => {
    const payload = event.payload
    const polygon = polygons.value.find(p => p.id === payload.polygonId)
    if (polygon) setProgressStatus(`${polygon.name}: ${payload.found}/${payload.needed ?? polygon.nbNeeded}`)
  }))
  unlisteners.push(await listen('generation:polygon-done', (event: any) => {
    const polygon = polygons.value.find(p => p.id === event.payload.polygonId)
    if (polygon) status.value = `${polygon.name} complete`
    persistPolygons()
  }))
  unlisteners.push(await listen('generation:finished', () => {
    running.value = false
    stopping.value = false
    status.value = `Finished · ${locationCount(markers.value.length)}`
    persistPolygons()
  }))

  window.addEventListener('beforeunload', persist)
  window.addEventListener('keydown', onKeyDown)

  // Closing the app works like Reset: locations and selected countries are
  // dropped, settings are kept.
  try {
    const appWindow = getCurrentWindow()
    unlistenClose = await appWindow.onCloseRequested(async event => {
      event.preventDefault()
      try {
        if (running.value) await stop()
        clearGenerationState()
      } finally {
        await appWindow.destroy()
      }
    })
  } catch (error) {
    console.warn('close handler unavailable', error)
  }

  try {
    const update = await check()
    if (update) {
      updateAvailable.value = true
      updateVersion.value = update.version
      updateHandle = update
    }
  } catch (error) {
    console.warn('update check failed', error)
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('beforeunload', persist)
  window.removeEventListener('keydown', onKeyDown)
  if (unlistenClose) unlistenClose()
  unlisteners.forEach(unlisten => unlisten())
  persistPolygons(true)
  if (map) map.remove()
})

function persist() {
  saveSettings(settings.value)
  persistPolygons(true)
}

let persistTimer: number | undefined

function persistPolygons(immediate = false) {
  if (immediate) {
    if (persistTimer) {
      window.clearTimeout(persistTimer)
      persistTimer = undefined
    }
    writePolygons()
    return
  }
  // Generation can emit hundreds of locations per second; writing the whole
  // list on every event wastes time, so coalesce the writes.
  if (persistTimer) return
  persistTimer = window.setTimeout(() => {
    persistTimer = undefined
    writePolygons()
  }, 400)
}

function writePolygons() {
  try {
    const slim = polygons.value.map(p => ({
      id: p.id,
      name: p.name,
      geometry: p.geometry,
      nbNeeded: p.nbNeeded,
      found: p.found,
    }))
    localStorage.setItem('mg:polygons', JSON.stringify(slim))
  } catch {}
}

async function loadSavedPolygons() {
  try {
    const raw = localStorage.getItem('mg:polygons')
    if (!raw) return
    const saved = JSON.parse(raw)
    if (!Array.isArray(saved)) return
    for (const polygon of saved) {
      polygons.value.push({
        id: polygon.id,
        name: polygon.name,
        geometry: polygon.geometry,
        nbNeeded: polygon.nbNeeded ?? 1000,
        found: [],
      })
      const record = polygons.value[polygons.value.length - 1]
      if (polygon.id?.startsWith('poly-')) addCustomLayer(polygon)
      for (const marker of polygon.found || []) {
        marker.polygonId = polygon.id
        markers.value.push(marker)
        record.found.push(marker)
        addMarkerToMap(marker)
      }
    }
    refreshCountryStyles()
    if (markers.value.length) status.value = `${locationCount(markers.value.length)} loaded`
  } catch (error) {
    console.warn('failed to load saved polygons', error)
  }
}

function providerLabel(provider: string) {
  return providerNames[provider] || provider
}

function addCustomLayer(polygon: Polygon) {
  // `renderer` is a Path option that the GeoJSON typings do not list.
  const options = {
    pane: 'customPane',
    renderer: customRenderer,
    interactive: false,
    style: { color: '#e07a3f', weight: 2, fillOpacity: 0.1 },
  }
  customLayer.addLayer(L.geoJSON(polygon.geometry as any, options))
}

function roadmapLayer(name: string): L.Layer {
  switch (name) {
    case 'default':
      return L.tileLayer(GOOGLE_ROADMAP, { maxZoom: 20 })
    case 'night':
      return L.tileLayer(GOOGLE_ROADMAP_NIGHT, { maxZoom: 20 })
    case 'classic':
      return L.tileLayer(GOOGLE_ROADMAP_CLASSIC, { maxZoom: 20 })
    case 'retro':
      return L.tileLayer(GOOGLE_ROADMAP_RETRO, { maxZoom: 20 })
    case 'aubergine':
      return L.tileLayer(GOOGLE_ROADMAP_AUBERGINE, { maxZoom: 20 })
    case 'satellite':
      return L.tileLayer(GOOGLE_SATELLITE, { maxZoom: 20 })
    case 'terrain':
      return L.tileLayer(GOOGLE_TERRAIN, { maxZoom: 20 })
    case 'carto':
      return L.tileLayer('https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png', { maxZoom: 20 })
    case 'osm':
      return L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 19 })
    default:
      return L.tileLayer(GOOGLE_ROADMAP_DARK, { maxZoom: 20 })
  }
}

function labelsLayer(name: string): L.Layer | null {
  switch (name) {
    case 'default':
      return L.tileLayer(GOOGLE_LABELS, { maxZoom: 20 })
    case 'night':
      return L.tileLayer(GOOGLE_LABELS_NIGHT, { maxZoom: 20 })
    case 'classic':
      return L.tileLayer(GOOGLE_LABELS_CLASSIC, { maxZoom: 20 })
    case 'retro':
      return L.tileLayer(GOOGLE_LABELS_RETRO, { maxZoom: 20 })
    case 'aubergine':
      return L.tileLayer(GOOGLE_LABELS_AUBERGINE, { maxZoom: 20 })
    case 'satellite':
      return L.tileLayer(GOOGLE_LABELS_SATELLITE, { maxZoom: 20 })
    case 'terrain':
      return L.tileLayer(GOOGLE_LABELS, { maxZoom: 20 })
    case 'carto':
    case 'osm':
      return null
    default:
      return L.tileLayer(GOOGLE_LABELS_DARK, { maxZoom: 20 })
  }
}

function setBase(name: string) {
  if (!map) return
  if (activeBase) map.removeLayer(activeBase)
  if (activeLabels) map.removeLayer(activeLabels)
  activeBase = roadmapLayer(name)
  activeLabels = labelsLayer(name)
  activeBase.addTo(map)
  if (coverageLayer) coverageLayer.addTo(map)
  if (activeLabels) activeLabels.addTo(map)
  settings.value.mapTheme = name
}

function applyCoverage() {
  if (!map) return
  if (coverageLayer) {
    map.removeLayer(coverageLayer)
    coverageLayer = null
  }
  const coverage = settings.value.coverage
  if (!coverage.enabled) return
  const url = streetViewUrl({
    official: coverage.official,
    blobby: coverage.blobby,
    scheme: coverage.colorScheme,
  })
  coverageLayer = L.tileLayer(url, {
    maxZoom: 20,
    minZoom: 3,
    opacity: coverage.opacity,
  })
  coverageLayer.addTo(map)
  if (activeLabels) {
    map.removeLayer(activeLabels)
    activeLabels.addTo(map)
  }
}

function onProviderChange() {
  if (settings.value.provider === 'mapycz') {
    status.value = 'Mapy.cz provider needs an API key'
  }
}

function toggleMarkerLayer() {
  if (!map) return
  if (markersVisible.value) markerLayer.addTo(map)
  else map.removeLayer(markerLayer)
}

async function loadCountries() {
  try {
    const response = await fetch('/geojson/world_borders.json')
    const data = await response.json()
    countryFeatures.value = data.features || []
    countryFeatures.value.forEach((feature, index) => {
      feature.properties = { ...(feature.properties || {}), __index: index }
    })
    countryLayer = L.geoJSON(data, {
      pane: 'countriesPane',
      style: featureStyle,
      onEachFeature: (feature, layer) => {
        const index = Number(feature.properties?.__index ?? -1)
        if (index >= 0) countryLayers.set(index, layer as L.GeoJSON)
        layer.on('click', (event: L.LeafletMouseEvent) => {
          if (drawing.value) return
          L.DomEvent.stopPropagation(event as any)
          if (index >= 0) toggleCountryByIndex(index)
        })
        layer.on('mouseover', () => {
          const selected = polygons.value.some(p => p.id === countryId(feature))
          if (!selected) (layer as L.Path).setStyle({ opacity: 0.55, weight: 1, fillOpacity: 0.08 })
        })
        layer.on('mouseout', () => {
          // Restore the shared style so a country selected while hovering keeps
          // its highlight.
          ;(layer as L.Path).setStyle(featureStyle(feature))
        })
      },
    }).addTo(map)
  } catch (error) {
    console.warn('failed to load countries', error)
  }
}

function countryId(feature: any) {
  return `country-${feature?.properties?.__index ?? feature?.properties?.code ?? ''}`
}

function featureStyle(feature: any) {
  const selected = polygons.value.some(p => p.id === countryId(feature))
  return {
    pane: 'countriesPane',
    color: selected ? '#e07a3f' : '#7d8794',
    weight: selected ? 1.5 : 1,
    // Unselected borders stay invisible; hovering reveals them.
    opacity: selected ? 0.9 : 0,
    fillColor: '#e07a3f',
    fillOpacity: selected ? 0.28 : 0,
  }
}

function refreshCountryStyles() {
  if (countryLayer) countryLayer.setStyle(featureStyle)
}

function pickCountry(index: number) {
  countryQuery.value = ''
  searchOpen.value = false
  const id = `country-${index}`
  if (!polygons.value.some(p => p.id === id)) toggleCountryByIndex(index)
  const layer = countryLayers.get(index)
  if (layer) map.fitBounds((layer as any).getBounds(), { padding: [20, 20], maxZoom: 7 })
}

function toggleCountryByIndex(index: number) {
  const feature = countryFeatures.value[index]
  if (!feature) return
  const id = `country-${index}`
  const existing = polygons.value.find(p => p.id === id)
  if (existing) {
    polygons.value = polygons.value.filter(p => p.id !== id)
    existing.found.forEach(marker => removeMarkerRecord(marker))
  } else {
    polygons.value.push({
      id,
      name: feature.properties?.name || feature.properties?.code || `Country ${index}`,
      geometry: feature.geometry,
      nbNeeded: 1000,
      found: [],
    })
  }
  refreshCountryStyles()
  persistPolygons()
}

function onMapClick(event: L.LeafletMouseEvent) {
  if (!drawing.value) return
  drawPoints.value.push(event.latlng)
  const dot = L.circleMarker(event.latlng, {
    pane: 'markersPane', renderer: markerRenderer, radius: 4, color: '#e07a3f', fillColor: '#e07a3f', fillOpacity: 1,
  }).addTo(map)
  drawMarkers.value.push(dot)
  if (drawLine.value) map.removeLayer(drawLine.value as any)
  drawLine.value = L.polyline(drawPoints.value, {
    pane: 'customPane', renderer: customRenderer, interactive: false, color: '#e07a3f', weight: 2,
  }).addTo(map)
}

function toggleDraw() {
  if (drawing.value) finishDraw()
  else {
    drawing.value = true
    drawPoints.value = []
    map.doubleClickZoom.disable()
    status.value = 'Click to place vertices · double-click to finish'
  }
}

function finishDraw() {
  if (!drawing.value) return
  if (drawPoints.value.length >= 3) {
    const coordinates = drawPoints.value.map(point => [point.lng, point.lat])
    coordinates.push(coordinates[0])
    const geometry = { type: 'Polygon', coordinates: [coordinates] }
    const polygon: Polygon = {
      id: `poly-${Date.now()}`,
      name: `Custom polygon ${polygons.value.filter(p => p.id.startsWith('poly-')).length + 1}`,
      geometry,
      nbNeeded: 1000,
      found: [],
    }
    polygons.value.push(polygon)
    addCustomLayer(polygon)
    persistPolygons()
  }
  cancelDrawing()
  status.value = 'Ready'
}

/** Drops the in-progress draw overlays and leaves draw mode. */
function cancelDrawing() {
  drawMarkers.value.forEach(dot => map.removeLayer(dot as any))
  drawMarkers.value = []
  if (drawLine.value) {
    map.removeLayer(drawLine.value as any)
    drawLine.value = null
  }
  drawPoints.value = []
  if (drawing.value) {
    drawing.value = false
    map.doubleClickZoom.enable()
  }
}

function removePolygon(id: string) {
  const polygon = polygons.value.find(p => p.id === id)
  if (polygon) polygon.found.forEach(marker => removeMarkerRecord(marker))
  polygons.value = polygons.value.filter(p => p.id !== id)
  if (id.startsWith('poly-')) {
    customLayer.clearLayers()
    polygons.value.filter(p => p.id.startsWith('poly-')).forEach(addCustomLayer)
  }
  refreshCountryStyles()
  persistPolygons()
}

function clearPolygons() {
  polygons.value.forEach(p => p.found.forEach(marker => removeMarkerRecord(marker)))
  polygons.value = []
  customLayer.clearLayers()
  refreshCountryStyles()
  persistPolygons()
}

async function importGeoJson() {
  try {
    const selected = await openDialog({ filters: [{ name: 'GeoJSON', extensions: ['json', 'geojson'] }] })
    if (!selected || Array.isArray(selected)) return
    const text = await invoke<string>('read_text_file', { path: selected })
    const data = JSON.parse(text)
    const features = data.type === 'FeatureCollection' ? data.features : [data]
    for (const feature of features) {
      const geometry = feature.geometry || feature
      if (!geometry || !geometry.type) continue
      const polygon: Polygon = {
        id: `poly-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        name: feature.properties?.name || `Imported ${polygons.value.length + 1}`,
        geometry,
        nbNeeded: 1000,
        found: [],
      }
      polygons.value.push(polygon)
      addCustomLayer(polygon)
    }
    status.value = `Imported ${features.length} polygon(s)`
    persistPolygons()
  } catch (error) {
    status.value = `Import failed: ${String(error)}`
  }
}

function setNeeded(polygon: Polygon, event: Event) {
  const value = Number((event.target as HTMLInputElement).value)
  polygon.nbNeeded = Math.max(polygon.found.length, Number.isFinite(value) ? value : 0)
  ;(event.target as HTMLInputElement).value = String(polygon.nbNeeded)
  persistPolygons()
}

function markerColour(marker: Marker) {
  switch (marker.markerType) {
    case 'gen1': return '#24ac20'
    case 'gen2or3': return '#9a28ca'
    case 'gen4': return '#2880ca'
    case 'newRoad': return '#ca283f'
    case 'noBlueLine': return '#e412d2'
    default: return '#e07a3f'
  }
}

function markerVisible(marker: Marker) {
  switch (marker.markerType) {
    case 'gen1': return settings.value.markers.gen1
    case 'gen2or3': return settings.value.markers.gen2Or3
    case 'gen4': return settings.value.markers.gen4
    case 'newRoad': return settings.value.markers.newRoad
    case 'noBlueLine': return settings.value.markers.noBlueLine
    default: return true
  }
}

function addMarkerToMap(marker: MarkerRecord) {
  const colour = markerColour(marker)
  const layer = L.circleMarker([marker.lat, marker.lng], {
    pane: 'markersPane',
    renderer: markerRenderer,
    radius: 5,
    color: '#0d0f11',
    weight: 1,
    fillColor: colour,
    fillOpacity: 1,
  })
  const date = marker.imageDate || 'unknown date'
  const extra = marker.updateType ? ` · ${marker.updateType}` : ''
  layer.bindTooltip(`${date}${extra}`, { direction: 'top', offset: [0, -6] })
  layer.on('click', () => openPanorama(marker))
  markerLayers.push({ marker, layer })
  if (markerVisible(marker)) layer.addTo(markerLayer)
}

function removeMarkerRecord(marker: MarkerRecord) {
  // `marker` can be a reactive proxy (from `polygon.found`) while
  // `markerLayers` holds the raw objects, so compare against the raw value.
  const raw = toRaw(marker)
  const index = markers.value.indexOf(raw as MarkerRecord)
  if (index >= 0) markers.value.splice(index, 1)
  const entryIndex = markerLayers.findIndex(entry => toRaw(entry.marker) === raw)
  if (entryIndex >= 0) {
    markerLayer.removeLayer(markerLayers[entryIndex].layer)
    markerLayers.splice(entryIndex, 1)
  }
}

function refreshMarkerVisibility() {
  markerLayers.forEach(({ marker, layer }) => {
    if (markerVisible(marker)) layer.addTo(markerLayer)
    else markerLayer.removeLayer(layer)
  })
  saveSettings(settings.value)
}

function openPanorama(marker: Marker) {
  const provider = (marker.source || '').replace('_pano', '').replace('qq', 'tencent')
  const heading = marker.heading ?? 0
  const pitch = marker.pitch ?? 0
  const zoom = marker.zoom ?? 0
  let url = ''
  switch (provider) {
    case 'google':
      url = `https://www.google.com/maps/@?api=1&map_action=pano&pano=${marker.panoId}&heading=${heading}&pitch=${pitch}&fov=${180 / 2 ** zoom}`
      break
    case 'yandex':
      url = `https://yandex.com/maps/?l=stv%2Csta&ll=${marker.lng},${marker.lat}&panorama%5Bdirection%5D=${heading},0&panorama%5Bfull%5D=true&panorama%5Bid%5D=${marker.panoId}&panorama%5Bpoint%5D=${marker.lng},${marker.lat}`
      break
    case 'tencent':
      url = `https://qq-map.netlify.app/#base=roadmap&zoom=18&center=${marker.lat},${marker.lng}&pano=${marker.panoId}&heading=${heading}&pitch=${pitch}&svz=0`
      break
    case 'baidu':
      url = `https://map.baidu.com/?newmap=1&shareurl=1&panotype=street&l=21&tn=B_NORMAL_MAP&sc=0&panoid=${marker.panoId}&heading=${heading}&pitch=${pitch}&pid=${marker.panoId}`
      break
    case 'apple':
      url = `https://lookmap.skzk.dev/#c=18/${marker.lat}/${marker.lng}&p=${marker.lat}/${marker.lng}&a=${heading}/${pitch}`
      break
    case 'bing':
      url = `https://www.bing.com/maps/?cc=cn&style=x&lvl=18&id=${marker.panoId}&cp=${marker.lat}%7E${marker.lng}&dir=${heading}&pi=${pitch}&setlang=en`
      break
    case 'kakao':
      url = `https://map.kakao.com/?map_type=TYPE_MAP&map_attribute=ROADVIEW&panoid=${marker.panoId}&pan=${heading}&tilt=${pitch}`
      break
    case 'naver':
      url = `https://map.naver.com/p?c=10.00,0,0,0,adh&p=${marker.panoId},${heading > 180 ? heading - 360 : heading},${pitch},80`
      break
    case 'mapycz':
      url = `https://mapy.cz/app?pid=${marker.panoId}&yaw=${heading}&pitch=${pitch}&x=${marker.lng}&y=${marker.lat}&z=15`
      break
    case 'mapillary':
      url = `https://www.mapillary.com/app/?lat=${marker.lat}&lng=${marker.lng}&z=15&pKey=${marker.panoId}&focus=photo&x=${heading}&y=${pitch}&zoom=${zoom}`
      break
    case 'openmap':
      url = `https://vn-map.netlify.app/#zoom=15&center=${marker.lat},${marker.lng}&pano=${marker.panoId}&ppos=${marker.lat},${marker.lng}&heading=${heading}&pitch=${pitch}`
      break
    case 'asig':
      url = `https://360.asig.gov.al/AlbaniaStreetView/player2/?sv_startup_pano=${marker.panoId}&sv_startup_heading=${heading}&sv_startup_tilt=&sv_startup_zoom=${zoom}&map_center=${marker.lat},${marker.lng}&map_zoom=15&v_lat=${marker.lat}&v_lng=${marker.lng}&vl_showshare=yes`
      break
    case 'vegbilder':
      url = `https://vegbilder.atlas.vegvesen.no/?lat=${marker.lat}&lng=${marker.lng}&zoom=15&view=image&imageId=${marker.panoId}&year=${marker.imageDate?.slice(0, 4) || ''}`
      break
    default:
      url = `https://www.google.com/maps/@?api=1&map_action=pano&pano=${marker.panoId}&heading=${heading}&pitch=${pitch}&fov=${180 / 2 ** zoom}`
  }
  openUrl(url).catch(error => { status.value = `Could not open Street View: ${String(error)}` })
}

function toggleRun() {
  if (running.value) stop()
  else start()
}

async function start() {
  if (!polygons.value.length) {
    status.value = 'Select at least one country or draw a polygon'
    return
  }
  if (!polygons.value.some(p => p.found.length < p.nbNeeded)) {
    status.value = 'All location goals are already met'
    return
  }
  running.value = true
  stopping.value = false
  status.value = 'Starting…'
  const polygonInput = polygons.value.map(p => ({
    id: p.id,
    name: p.name,
    geometry: p.geometry,
    nbNeeded: p.nbNeeded,
    found: p.found,
  }))
  try {
    await invoke('start_generation', { settings: settings.value, polygons: polygonInput })
  } catch (error) {
    running.value = false
    status.value = `Start failed: ${String(error)}`
  }
}

async function stop() {
  stopping.value = true
  try {
    await invoke('stop_generation')
  } catch (error) {
    console.warn('stop failed', error)
  }
  running.value = false
  stopping.value = false
  status.value = 'Stopped'
}

const confirmOpen = ref(false)
const clearAllOpen = ref(false)
const exportNotice = ref('')
let exportNoticeTimer: number | undefined

/** "1 location" / "24 locations". */
function locationCount(count: number) {
  return `${count} ${count === 1 ? 'location' : 'locations'}`
}

/** Short-lived confirmation shown under the export buttons. */
function showExportNotice(text: string) {
  exportNotice.value = text
  if (exportNoticeTimer) window.clearTimeout(exportNoticeTimer)
  exportNoticeTimer = window.setTimeout(() => {
    exportNotice.value = ''
  }, 2600)
}

let lastProgressAt = 0

/** Progress events can arrive faster than the UI needs to repaint. */
function setProgressStatus(text: string) {
  const now = Date.now()
  if (now - lastProgressAt < 120) return
  lastProgressAt = now
  status.value = text
}

async function resetGeneration() {
  if (running.value) await stop()
  confirmOpen.value = true
}

async function confirmReset() {
  confirmOpen.value = false
  clearGenerationState()
  status.value = 'Generation reset'
}

/** Clears countries, locations and drawings but keeps the settings. */
function clearGenerationState() {
  cancelDrawing()
  markers.value = []
  markerLayers.length = 0
  markerLayer.clearLayers()
  markersVisible.value = true
  markerLayer.addTo(map)
  polygons.value = []
  customLayer.clearLayers()
  refreshCountryStyles()
  // Write immediately: the app can be closing right after this.
  persistPolygons(true)
}

function cancelReset() {
  confirmOpen.value = false
}

function clearPolygonLocations(polygon: Polygon) {
  polygon.found.forEach(marker => removeMarkerRecord(marker))
  polygon.found = []
  status.value = `${polygon.name} cleared`
  persistPolygons()
}

function askClearAllLocations() {
  if (!markers.value.length) return
  clearAllOpen.value = true
}

function confirmClearAllLocations() {
  clearAllOpen.value = false
  polygons.value.forEach(polygon => {
    polygon.found.forEach(marker => removeMarkerRecord(marker))
    polygon.found = []
  })
  markers.value = []
  markerLayers.length = 0
  markerLayer.clearLayers()
  status.value = 'All locations cleared'
  persistPolygons()
}

function exportPanoId(marker: Marker) {
  if (settings.value.panoId === 'disable') return null
  if (settings.value.panoId === 'prefix') {
    const label = marker.source?.includes('qq')
      ? 'TENCENT'
      : (marker.source || '').replace('_pano', '').toUpperCase()
    return `${label}:${marker.panoId}`
  }
  return marker.panoId
}

function buildExport(markerList: MarkerRecord[]) {
  return markerList.map(marker => ({
    panoId: exportPanoId(marker),
    lat: marker.lat,
    lng: marker.lng,
    heading: marker.heading,
    pitch: marker.pitch,
    zoom: marker.zoom,
    country: marker.country,
    region: marker.region,
    locality: marker.locality,
    road: marker.road,
    imageDate: marker.imageDate,
    source: marker.source,
    updateType: marker.updateType,
    links: marker.links?.slice(0, 4) || [],
    extra: { tags: settings.value.tag ? marker.extra?.tags || [] : [] },
  }))
}

async function copyMarkers(markerList: MarkerRecord[]) {
  const text = JSON.stringify(buildExport(markerList), null, 2)
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    try {
      const area = document.createElement('textarea')
      area.value = text
      document.body.appendChild(area)
      area.select()
      document.execCommand('copy')
      document.body.removeChild(area)
    } catch {
      showExportNotice('Could not copy to the clipboard')
      return
    }
  }
  showExportNotice(`Copied ${locationCount(markerList.length)}`)
}

const exportFileName = computed(() => exportBaseName(markers.value))

function exportJson(markerList: MarkerRecord[]) {
  const name = `${exportBaseName(markerList)}.json`
  saveText(name, JSON.stringify(buildExport(markerList), null, 2), 'json')
}

function exportCsv(markerList: MarkerRecord[]) {
  const header = 'panoId,lat,lng,heading,pitch,zoom,imageDate,updateType,country,region,locality,road,source'
  const rows = buildExport(markerList).map(item => [
    item.panoId ?? '',
    item.lat,
    item.lng,
    item.heading,
    item.pitch,
    item.zoom,
    item.imageDate ?? '',
    item.updateType ?? '',
    item.country ?? '',
    item.region ?? '',
    item.locality ?? '',
    item.road ?? '',
    item.source ?? '',
  ].map(value => `"${String(value).replaceAll('"', '""')}"`).join(','))
  const name = `${exportBaseName(markerList)}.csv`
  saveText(name, [header, ...rows].join('\n'), 'csv')
}

/** "Estonia-1000-locations" (or "3-countries-1000-locations" for mixed exports). */
function exportBaseName(markerList: MarkerRecord[]) {
  const polygonNames = new Map(polygons.value.map(polygon => [polygon.id, polygon.name]))
  const countries = [...new Set(markerList
    .map(marker => ((marker.polygonId && polygonNames.get(marker.polygonId)) || marker.country || '').trim())
    .filter(Boolean))]
  const label = countries.length === 0
    ? 'All-countries'
    : countries.length === 1
      ? countries[0]
      : `${countries.length}-countries`
  return `${safeName(label)}-${markerList.length}-locations`
}

function safeName(name: string) {
  return name
    .replace(/[<>:"/\\|?*\u0000-\u001f]+/g, '-')
    .replace(/\s+/g, ' ')
    .replace(/^[\s.-]+|[\s.-]+$/g, '') || 'map-generator'
}

async function saveText(name: string, content: string, extension: string) {
  try {
    const path = await saveDialog({
      defaultPath: name,
      filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
    })
    if (!path) {
      showExportNotice('Save cancelled')
      return
    }
    await invoke('write_text_file', { path, content })
    showExportNotice(`Saved ${name}`)
    return
  } catch (error) {
    console.warn('native save failed, falling back to download', error)
  }
  const blob = new Blob([content], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = name
  anchor.click()
  URL.revokeObjectURL(url)
  showExportNotice(`Downloaded ${name}`)
}

function isGenerationChecked(key: string) {
  return !!settings.value.findByGeneration?.[generationProviderKey.value]?.[key]
}

function setGenerationChecked(key: string, event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  if (!settings.value.findByGeneration) settings.value.findByGeneration = {}
  if (!settings.value.findByGeneration[generationProviderKey.value]) {
    settings.value.findByGeneration[generationProviderKey.value] = {}
  }
  settings.value.findByGeneration[generationProviderKey.value][key] = checked
}

function onKeyDown(event: KeyboardEvent) {
  const target = event.target as HTMLElement
  const tag = target?.tagName?.toLowerCase()
  if (tag === 'input' || tag === 'select' || tag === 'textarea') return
  if (event.code === 'Space') {
    event.preventDefault()
    toggleRun()
  }
  if (event.code === 'Escape' && drawing.value) {
    drawPoints.value = []
    drawMarkers.value.forEach(dot => map.removeLayer(dot as any))
    drawMarkers.value = []
    if (drawLine.value) {
      map.removeLayer(drawLine.value as any)
      drawLine.value = null
    }
    drawing.value = false
    map.doubleClickZoom.enable()
    status.value = 'Drawing cancelled'
  }
}

async function onNotificationToggle() {
  if (!settings.value.notification.enabled) return
  try {
    const granted = await notificationGranted()
    if (!granted) await requestNotificationPermission()
  } catch (error) {
    console.warn('notification permission failed', error)
  }
}

async function installUpdate() {
  if (!updateHandle) return
  updating.value = true
  updateProgress.value = 0
  updateError.value = ''
  status.value = 'Downloading update…'
  let total = 0
  let downloaded = 0
  try {
    await updateHandle.downloadAndInstall((event: any) => {
      if (event.event === 'Started') {
        total = event.data?.contentLength ?? 0
        updateProgress.value = 0
      } else if (event.event === 'Progress') {
        downloaded += event.data?.chunkLength ?? 0
        if (total > 0) {
          updateProgress.value = Math.min(100, Math.round((downloaded / total) * 100))
        }
      } else if (event.event === 'Finished') {
        updateProgress.value = 100
      }
    })
    status.value = 'Update installed, restarting…'
    await relaunch()
  } catch (error: any) {
    console.error('update install failed', error)
    updateError.value = String(error?.message || error)
    status.value = 'Update failed: ' + updateError.value
  } finally {
    updating.value = false
  }
}
</script>
