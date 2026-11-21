<template>
  <div class="app-shell">
    <div id="map"></div>

    <div class="columns">
      <div class="column left">
        <div class="logo">
          <span>Map Generator</span>
          <span class="version">v{{ appVersion }}</span>
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
              <div class="field">
                <span>Send to</span>
                <select v-model="settings.notification.target" @change="onNotificationToggle">
                  <option value="desktop">Desktop</option>
                  <option value="discord" :disabled="!isGoogle">Discord</option>
                  <option value="both" :disabled="!isGoogle">Both</option>
                </select>
              </div>
              <div class="field" v-if="isGoogle && settings.notification.target !== 'desktop'">
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
          <summary>Selected areas <span class="badge">{{ polygons.length }}</span></summary>
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
            <div class="toolbar">
              <button class="tool primary" @mousedown.prevent @click="toggleDraw">
                {{ drawing ? 'Finish drawing' : (editing ? 'Finish editing' : 'Draw') }}
              </button>
              <button v-if="drawing" class="tool" @mousedown.prevent @click="removeLastDrawPoint" :disabled="!drawPoints.length">Undo point</button>
              <button v-if="editing" class="tool" @mousedown.prevent @click="cancelEdit()">Cancel edit</button>
              <button class="tool" @mousedown.prevent @click="importGeoJson">Import</button>
              <button class="tool danger" @mousedown.prevent @click="clearPolygons" :disabled="!polygons.length">Deselect all</button>
            </div>

            <div class="empty-state" v-if="!polygons.length">
              <div class="empty-title">No areas selected</div>
              <div class="empty-text">Click a country on the map, search above, or draw your own polygon.</div>
            </div>

            <div class="area-list" v-else>
              <div class="area-card" v-for="p in polygons" :key="p.id">
                <div class="area-head">
                  <span class="area-flag" v-if="p.countryCode">
                    <img class="flag" :src="flagUrl(p.countryCode)" :alt="p.countryCode" @error="onFlagError" />
                    <span class="flag-code">{{ p.countryCode }}</span>
                  </span>
                  <span class="area-name" :title="p.name" @click="p.id.startsWith('poly-') ? renamePolygon(p) : pickCountry(Number(p.id.replace('country-','')))" >{{ p.name }}</span>
                </div>
                <div class="area-body">
                  <label class="goal-field">
                    <span>Goal</span>
                    <input type="number" min="0" v-model.number="p.nbNeeded" @change="onGoalChange(p)" />
                  </label>
                  <span class="area-count">{{ p.found.length }} / {{ p.nbNeeded }}</span>
                </div>
                <div class="area-actions">
                  <button class="chip" :disabled="!p.found.length" @click="clearPolygonLocations(p)">Clear</button>
                  <button class="chip" v-if="p.id.startsWith('poly-')" @click="askSavePolygon(p)">Save</button>
                  <button class="chip" v-if="p.id.startsWith('poly-')" @click="startEdit(p)">Edit</button>
                  <button class="chip danger" @click="askRemovePolygon(p)">{{ p.id.startsWith('poly-') ? 'Delete' : 'Remove' }}</button>
                </div>
              </div>
            </div>
            <div class="actions" v-if="markers.length">
              <button class="danger clear-all" @click="askClearAllLocations">Clear all locations</button>
            </div>
          </div>
        </details>

        <details class="section" open>
          <summary>Saved polygons <span class="badge">{{ savedPolygons.length }}</span></summary>
          <div class="body">
            <div class="search-wrap" v-if="savedPolygons.length">
              <input
                type="text"
                v-model="savedPolygonQuery"
                placeholder="Search saved polygons…"
                @keydown.esc="savedPolygonQuery = ''"
              />
            </div>
            <div class="hint" v-if="!savedPolygons.length">No saved polygons yet. Draw or import one, then use Save.</div>
            <div class="area-list saved-list" v-else-if="filteredSavedPolygons.length">
              <div class="area-card saved-card" v-for="sp in filteredSavedPolygons" :key="sp.id">
                <div class="area-head">
                  <span class="area-name" :title="sp.name" @click="renameSavedPolygon(sp)">{{ sp.name }}</span>
                </div>
                <div class="area-actions">
                  <button class="chip" @click="loadSavedPolygon(sp)">Load</button>
                  <button class="chip danger" @click="askDeleteSavedPolygon(sp)">Delete</button>
                </div>
              </div>
            </div>
            <div class="hint" v-else>No matches</div>
          </div>
        </details>

        <details class="section" open>
          <summary>Export all <span class="badge">{{ markers.length }}</span></summary>
          <div class="body">
            <div class="actions">
              <button :disabled="!markers.length || copying" @click="copyMarkers(markers)">
                <span v-if="copying" class="btn-spinner" aria-hidden="true"></span>
                Copy JSON
              </button>
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
            <label class="check" title="Skip random points in open water using a bundled land mask">
              <input type="checkbox" v-model="settings.useLandMask" /> Skip ocean samples
            </label>
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
              <div class="field" v-if="settings.checkLinks" title="Maximum linked panoramas checked between accepted locations. The counter resets after each match.">
                <span>Link look-ahead</span>
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

    <div v-if="running" class="running-indicator" aria-live="polite">
      <span class="spinner"></span>
      <span>{{ stopping ? 'Stopping…' : 'Generating' }} · {{ formatElapsed(runElapsed) }}</span>
    </div>

    <div v-if="confirmOpen" class="modal-backdrop" @click="cancelReset">
      <div class="modal" @click.stop>
        <div class="modal-title">Reset generation?</div>
        <div class="modal-body">This clears all found locations. Selected countries and their goals are kept. Settings are kept.</div>
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

    <div v-if="removePolyOpen" class="modal-backdrop" @click="cancelRemovePolygon">
      <div class="modal" @click.stop>
        <div class="modal-title">Remove polygon?</div>
        <div class="modal-body">
          This removes <strong>{{ polygonToRemove?.name }}</strong> and its {{ polygonToRemove ? locationCount(polygonToRemove.found.length) : '' }} from the map.
        </div>
        <div class="modal-actions">
          <button @click="cancelRemovePolygon">Cancel</button>
          <button class="danger" @click="confirmRemovePolygon">Remove</button>
        </div>
      </div>
    </div>

    <div v-if="savePolyOpen" class="modal-backdrop" @click="cancelSavePolygon">
      <div class="modal" @click.stop>
        <div class="modal-title">Save polygon</div>
        <div class="modal-body">
          <input v-model="saveName" type="text" placeholder="Polygon name" @keydown.enter.prevent="confirmSavePolygon" />
          <div v-if="saveError" class="modal-error">{{ saveError }}</div>
        </div>
        <div class="modal-actions">
          <button @click="cancelSavePolygon">Cancel</button>
          <button class="primary" @click="confirmSavePolygon">Save</button>
        </div>
      </div>
    </div>

    <div v-if="renameOpen" class="modal-backdrop" @click="cancelRename">
      <div class="modal" @click.stop>
        <div class="modal-title">Rename polygon</div>
        <div class="modal-body">
          <input v-model="renameName" type="text" placeholder="Polygon name" @keydown.enter.prevent="confirmRename" />
          <div v-if="renameError" class="modal-error">{{ renameError }}</div>
        </div>
        <div class="modal-actions">
          <button @click="cancelRename">Cancel</button>
          <button class="primary" @click="confirmRename">Rename</button>
        </div>
      </div>
    </div>

    <div v-if="deleteSavedOpen" class="modal-backdrop" @click="cancelDeleteSavedPolygon">
      <div class="modal" @click.stop>
        <div class="modal-title">Delete saved polygon?</div>
        <div class="modal-body">
          This removes <strong>{{ savedPolygonToDelete?.name }}</strong> from your saved polygons. It does not affect polygons currently on the map.
        </div>
        <div class="modal-actions">
          <button @click="cancelDeleteSavedPolygon">Cancel</button>
          <button class="danger" @click="confirmDeleteSavedPolygon">Delete</button>
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
import { computed, markRaw, onBeforeUnmount, onMounted, ref, shallowRef, toRaw, watch } from 'vue'
import L from 'leaflet'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog'
import { openUrl } from '@tauri-apps/plugin-opener'
import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import versionJson from '../version.json'
import {
  isPermissionGranted as notificationGranted,
  requestPermission as requestNotificationPermission,
} from '@tauri-apps/plugin-notification'
import type { Marker, PanoDotTile, Polygon, Settings } from './types'
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

const appVersion = versionJson.version

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
const PANO_DOTS_ZOOM = 17
const PANO_DOT_TILE_ZOOM = 17
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
const runElapsed = ref(0)
/** Marks the live page so a reload can be recognised as a refresh. */
const SESSION_KEY = 'mg:session'
const POLYGONS_KEY = 'mg:polygons'
const SAVED_POLYGONS_KEY = 'mg:saved-polygons'
let runStartedAt = 0
let elapsedTimer: number | undefined
const status = ref('Ready')
const drawing = ref(false)
// Leaflet layers must stay raw: Vue's deep proxies break their internal
// identity checks and they stop following zoom/pan.
const drawPoints = shallowRef<L.LatLng[]>([])
const drawLine = shallowRef<L.Polyline | null>(null)
const drawMarkers = shallowRef<L.CircleMarker[]>([])
const editing = ref<Polygon | null>(null)
const editMarkers = shallowRef<L.Marker[]>([])
const editPoints = shallowRef<L.LatLng[]>([])
const editLine = shallowRef<L.Polygon | null>(null)
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
let panoDotLayer: L.LayerGroup
// Markers and drawn overlays use SVG renderers: a Canvas renderer fills the
// whole pane and swallows every map click, which made country selection stop
// working as soon as one marker existed.
let markerRenderer: L.SVG
let customRenderer: L.SVG
let panoRenderer: L.SVG
let countryLayer: L.GeoJSON | null = null
let activeBase: L.Layer | null = null
let activeLabels: L.Layer | null = null
let coverageLayer: L.TileLayer | null = null
// base cache no longer used
const markerLayers: Array<{ marker: MarkerRecord; layer: L.CircleMarker }> = []
const panoDotTiles = new Map<string, PanoDotTile['dots']>()
const panoDotFetching = new Set<string>()
const seenMarkers = new Set<string>()
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
watch(isGoogle, google => {
  if (!google && settings.value.notification.target !== 'desktop') settings.value.notification.target = 'desktop'
})
watch(() => polygons.value.map(p => p.id).join(','), refreshCountryStyles)
watch(() => settings.value.coverage.opacity, value => { if (coverageLayer) coverageLayer.setOpacity(value) })

onMounted(async () => {
  // sessionStorage survives a refresh but not a relaunch, which is how a
  // refresh is told apart from a normal start. Refreshing behaves exactly like
  // Reset: the previous run is discarded instead of being picked back up.
  const refreshed = sessionStorage.getItem(SESSION_KEY) === '1'
  sessionStorage.setItem(SESSION_KEY, '1')

  // A refresh recreates the page but not the backend, so a run from the
  // previous page can still be going. Stop it before anything else.
  try {
    await invoke('stop_generation')
  } catch (error) {
    console.warn('stop on load failed', error)
  }

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
  map.createPane('panoPointsPane')
  map.getPane('panoPointsPane')!.style.zIndex = '660'
  // The dots are non-interactive; never let their layer eat map clicks.
  map.getPane('panoPointsPane')!.style.pointerEvents = 'none'

  // Canvas rendering keeps thousands of pins cheap; SVG chokes on large sets.
  markerRenderer = L.canvas({ pane: 'markersPane' })
  customRenderer = L.svg({ pane: 'customPane' })
  panoRenderer = L.svg({ pane: 'panoPointsPane' })
  markerLayer = L.layerGroup().addTo(map)
  customLayer = L.layerGroup().addTo(map)
  panoDotLayer = L.layerGroup().addTo(map)

  setBase(settings.value.mapTheme || 'default')
  applyCoverage()
  map.on('zoomend', () => {
    currentZoom.value = map.getZoom()
    void refreshPanoDots()
  })
  map.on('moveend', () => { void refreshPanoDots() })
  map.on('click', onMapClick)
  map.on('dblclick', onDblClick)

  await loadCountries()
  loadSavedPolygonsLibrary()
  if (refreshed) {
    // Drop the snapshots the previous page wrote, so nothing is restored.
    try {
      localStorage.removeItem(POLYGONS_KEY)
    } catch {}
  } else {
    await loadSavedPolygons()
  }

  unlisteners.push(await listen('generation:location', (event: any) => {
    const payload = event.payload
    // Drop events that arrive after a stop has been requested.
    if (stopping.value) return
    const polygon = polygons.value.find(p => p.id === payload.polygonId)
    // Late events from a stopped or reset run are dropped: the polygon is gone.
    if (!polygon) return
    const marker = payload.location as MarkerRecord
    const key = `${marker.panoId ?? ''}|${marker.source ?? ''}`
    if (seenMarkers.has(key)) return
    seenMarkers.add(key)
    marker.polygonId = payload.polygonId
    markers.value.push(marker)
    addMarkerToMap(marker)
    polygon.found.push(marker)
    setProgressStatus(`${polygon.name}: ${polygon.found.length}/${polygon.nbNeeded}`)
    // Persistence is coalesced and deferred to avoid serialising on every pin.
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
    // A run that was still going when the page was refreshed is stopped on
    // load; ignore the tail of that run.
    if (!running.value) return
    stopElapsedTimer()
    running.value = false
    stopping.value = false
    status.value = `Finished · ${locationCount(markers.value.length)}`
    persistPolygons()
  }))

  window.addEventListener('beforeunload', endSession)
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
  stopElapsedTimer()
  window.removeEventListener('beforeunload', endSession)
  window.removeEventListener('keydown', onKeyDown)
  if (unlistenClose) unlistenClose()
  unlisteners.forEach(unlisten => unlisten())
  persistPolygons(true)
  if (map) map.remove()
})

/**
 * Leaving the page (refresh, Ctrl+R) ends the session exactly like closing the
 * window: the backend run is stopped, locations and selected countries are
 * dropped, settings are kept.
 */
function endSession() {
  void invoke('stop_generation').catch(() => {})
  try {
    // Writes the empty polygon list to localStorage, so the reloaded page does
    // not restore the run that was just discarded.
    if (map) clearGenerationState()
  } catch (error) {
    console.warn('clear on unload failed', error)
  }
  saveSettings(settings.value)
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
  }, 1500)
}

function writePolygons() {
  try {
    const slim = polygons.value.map(p => ({
      id: p.id,
      name: p.name,
      geometry: p.geometry,
      nbNeeded: p.nbNeeded,
      found: p.found,
      countryCode: p.countryCode,
    }))
    localStorage.setItem(POLYGONS_KEY, JSON.stringify(slim))
  } catch {}
}

async function loadSavedPolygons() {
  try {
    const raw = localStorage.getItem(POLYGONS_KEY)
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
        countryCode: polygon.countryCode,
      })
      const record = polygons.value[polygons.value.length - 1]
      if (!polygon.id?.startsWith('country-')) addCustomLayer(polygon)
      for (const marker of polygon.found || []) {
        marker.polygonId = polygon.id
        seenMarkers.add(`${marker.panoId ?? ''}|${marker.source ?? ''}`)
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

function loadSavedPolygonsLibrary() {
  try {
    const raw = localStorage.getItem(SAVED_POLYGONS_KEY)
    if (!raw) return
    const saved = JSON.parse(raw)
    if (Array.isArray(saved)) savedPolygons.value = saved
  } catch (error) {
    console.warn('failed to load saved polygons library', error)
  }
}

function persistSavedPolygons() {
  try {
    localStorage.setItem(SAVED_POLYGONS_KEY, JSON.stringify(savedPolygons.value))
  } catch {}
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
  if (markersVisible.value) {
    markerLayer.addTo(map)
    if (map.getZoom() >= PANO_DOTS_ZOOM) panoDotLayer.addTo(map)
    void refreshPanoDots()
  } else {
    map.removeLayer(markerLayer)
    map.removeLayer(panoDotLayer)
  }
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
          if (drawing.value) return
          const selected = polygons.value.some(p => p.id === countryId(feature))
          if (!selected) (layer as L.Path).setStyle({ opacity: 0.55, weight: 1, fillOpacity: 0.08 })
        })
        layer.on('mouseout', () => {
          if (drawing.value) return
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
      countryCode: feature.properties?.code,
    })
  }
  refreshCountryStyles()
  persistPolygons()
}

function onMapClick(event: L.LeafletMouseEvent) {
  if (editing.value) return
  if (!drawing.value) return
  // Finish by clicking near the first vertex.
  if (drawPoints.value.length >= 3) {
    const first = drawPoints.value[0]
    const a = map.latLngToContainerPoint(first)
    const b = map.latLngToContainerPoint(event.latlng)
    if (Math.hypot(a.x - b.x, a.y - b.y) <= 16) {
      finishDraw()
      return
    }
  }
  // shallowRef needs a new array for template updates (e.g. the undo button).
  drawPoints.value = [...drawPoints.value, markRaw(event.latlng)]
  const isFirst = drawPoints.value.length === 1
  const dot = markRaw(L.circleMarker(event.latlng, {
    pane: 'markersPane', renderer: markerRenderer,
    radius: isFirst ? 9 : 4,
    color: '#e07a3f', fillColor: isFirst ? '#ffffff' : '#e07a3f', fillOpacity: 1,
    weight: isFirst ? 2.5 : 1,
  }).addTo(map))
  if (isFirst) {
    dot.on('click', (e: any) => { L.DomEvent.stopPropagation(e); finishDraw() })
  }
  drawMarkers.value = [...drawMarkers.value, dot]
  if (drawLine.value) {
    drawLine.value.setLatLngs([...drawPoints.value])
  } else {
    drawLine.value = markRaw(L.polyline([...drawPoints.value], {
      pane: 'customPane', renderer: customRenderer, interactive: false, color: '#e07a3f', weight: 2,
    }).addTo(map))
  }
  status.value = `Drawing · ${drawPoints.value.length} vertices`
}

/** Removes the most recently placed vertex while drawing. */
function removeLastDrawPoint() {
  if (!drawing.value || !drawPoints.value.length) return
  const dot = drawMarkers.value[drawMarkers.value.length - 1]
  drawMarkers.value = drawMarkers.value.slice(0, -1)
  drawPoints.value = drawPoints.value.slice(0, -1)
  if (dot) map.removeLayer(dot as any)
  if (!drawPoints.value.length) {
    if (drawLine.value) {
      map.removeLayer(drawLine.value as any)
      drawLine.value = null
    }
  } else if (drawLine.value) {
    drawLine.value.setLatLngs([...drawPoints.value])
  }
  status.value = drawPoints.value.length
    ? `Drawing · ${drawPoints.value.length} vertices`
    : 'Click to place vertices · click first point or double-click to finish · Backspace to undo'
}

function toggleDraw() {
  if (drawing.value) finishDraw()
  else if (editing.value) finishEdit()
  else {
    drawing.value = true
    drawPoints.value = []
    map.doubleClickZoom.disable()
    map.getContainer().classList.add('is-drawing')
    map.getContainer().style.cursor = 'crosshair'
    status.value = 'Click to place vertices · click first point or double-click to finish · Backspace to undo'
  }
}

function onDblClick() {
  if (drawing.value) finishDraw()
  else if (editing.value) finishEdit()
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
    map.getContainer().classList.remove('is-drawing')
    map.getContainer().style.cursor = ''
  }
}

function askRemovePolygon(polygon: Polygon) {
  // Built-in countries are a simple toggle; custom polygons need confirmation.
  if (!polygon.id.startsWith('poly-')) {
    removePolygon(polygon.id)
    return
  }
  polygonToRemove.value = polygon
  removePolyOpen.value = true
}

function cancelRemovePolygon() {
  removePolyOpen.value = false
  polygonToRemove.value = null
}

function confirmRemovePolygon() {
  const id = polygonToRemove.value?.id
  cancelRemovePolygon()
  if (id) removePolygon(id)
}

function restoreCustomLayers() {
  customLayer.clearLayers()
  polygons.value.filter(p => !p.id.startsWith('country-')).forEach(addCustomLayer)
}

function renamePolygon(polygon: Polygon) {
  renameSessionPolygon.value = polygon
  renameSavedPolygonTarget.value = null
  renameName.value = polygon.name
  renameError.value = ''
  renameOpen.value = true
}

function renameSavedPolygon(saved: any) {
  renameSessionPolygon.value = null
  renameSavedPolygonTarget.value = saved
  renameName.value = saved.name
  renameError.value = ''
  renameOpen.value = true
}

function cancelRename() {
  renameOpen.value = false
  renameSessionPolygon.value = null
  renameSavedPolygonTarget.value = null
  renameName.value = ''
  renameError.value = ''
}

function confirmRename() {
  const name = renameName.value.trim()
  if (renameSavedPolygonTarget.value && name) {
    const duplicate = savedPolygons.value.some(item =>
      item !== renameSavedPolygonTarget.value && String(item.name).toLowerCase() === name.toLowerCase())
    if (duplicate) {
      renameError.value = `A saved polygon named "${name}" already exists`
      return
    }
  }
  if (renameSessionPolygon.value) {
    if (name) renameSessionPolygon.value.name = name
    persistPolygons()
  } else if (renameSavedPolygonTarget.value) {
    if (name) renameSavedPolygonTarget.value.name = name
    persistSavedPolygons()
  }
  const finalName = name || renameSessionPolygon.value?.name || renameSavedPolygonTarget.value?.name || ''
  status.value = `Renamed to "${finalName}"`
  cancelRename()
}

function removePolygon(id: string) {
  if (editing.value?.id === id) cancelEdit()
  const polygon = polygons.value.find(p => p.id === id)
  if (polygon) polygon.found.forEach(marker => removeMarkerRecord(marker))
  polygons.value = polygons.value.filter(p => p.id !== id)
  if (!id.startsWith('country-')) restoreCustomLayers()
  refreshCountryStyles()
  persistPolygons()
}

function askSavePolygon(polygon: Polygon) {
  polygonToSave.value = polygon
  saveName.value = polygon.name
  saveError.value = ''
  savePolyOpen.value = true
}

function cancelSavePolygon() {
  savePolyOpen.value = false
  polygonToSave.value = null
  saveName.value = ''
  saveError.value = ''
}

function confirmSavePolygon() {
  if (!polygonToSave.value) return
  const name = saveName.value.trim() || polygonToSave.value.name
  if (savedPolygons.value.some(item => String(item.name).toLowerCase() === name.toLowerCase())) {
    saveError.value = `A saved polygon named "${name}" already exists`
    return
  }
  savedPolygons.value.push({
    id: `saved-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    name,
    geometry: polygonToSave.value.geometry,
    nbNeeded: polygonToSave.value.nbNeeded ?? 1000,
  })
  persistSavedPolygons()
  cancelSavePolygon()
  status.value = `Saved "${name}"`
}

function loadSavedPolygon(saved: any) {
  const polygon: Polygon = {
    id: `area-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    name: saved.name,
    geometry: saved.geometry,
    nbNeeded: saved.nbNeeded ?? 1000,
    found: [],
  }
  polygons.value.push(polygon)
  addCustomLayer(polygon)
  refreshCountryStyles()
  persistPolygons()
  status.value = `Loaded "${saved.name}"`
}


function askDeleteSavedPolygon(saved: any) {
  savedPolygonToDelete.value = saved
  deleteSavedOpen.value = true
}

function cancelDeleteSavedPolygon() {
  deleteSavedOpen.value = false
  savedPolygonToDelete.value = null
}

function confirmDeleteSavedPolygon() {
  const id = savedPolygonToDelete.value?.id
  cancelDeleteSavedPolygon()
  if (id) {
    savedPolygons.value = savedPolygons.value.filter(item => item.id !== id)
    persistSavedPolygons()
    status.value = 'Saved polygon deleted'
  }
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

/** Last goal the engine was told about, per polygon, for live goal edits. */
const goalSentToEngine = new Map<string, number>()

function onGoalChange(polygon: Polygon) {
  if (!Number.isFinite(polygon.nbNeeded) || polygon.nbNeeded < 0) polygon.nbNeeded = 0
  polygon.nbNeeded = Math.max(polygon.found.length, polygon.nbNeeded)
  persistPolygons()
  if (running.value) {
    const sent = goalSentToEngine.get(polygon.id) ?? polygon.nbNeeded
    // Raising a goal after its generators have exited needs a fresh run;
    // otherwise the engine applies the new goal live, without restarting.
    if (polygon.found.length >= sent && polygon.nbNeeded > sent) {
      start()
    } else {
      invoke('update_goals', { goals: [{ id: polygon.id, nbNeeded: polygon.nbNeeded }] })
        .then(() => goalSentToEngine.set(polygon.id, polygon.nbNeeded))
        .catch(error => console.warn('goal update failed', error))
    }
  }
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

function panoTileKey(tile: { x: number; y: number }) {
  return `${tile.x},${tile.y}`
}

function panoTileFor(lat: number, lng: number) {
  const scale = 2 ** PANO_DOT_TILE_ZOOM
  const clampedLat = Math.max(-85.05112878, Math.min(85.05112878, lat))
  const sin = Math.sin((clampedLat * Math.PI) / 180)
  const x = Math.floor(((lng + 180) / 360) * scale)
  const y = Math.floor((0.5 - Math.log((1 + sin) / (1 - sin)) / (4 * Math.PI)) * scale)
  return {
    x: ((x % scale) + scale) % scale,
    y: Math.max(0, Math.min(scale - 1, y)),
  }
}

function visiblePanoTiles() {
  const scale = 2 ** PANO_DOT_TILE_ZOOM
  const bounds = map.getBounds()
  let west = Math.floor(((bounds.getWest() + 180) / 360) * scale)
  let east = Math.floor(((bounds.getEast() + 180) / 360) * scale)
  if (east < west) east += scale
  const north = panoTileFor(bounds.getNorth(), 0).y
  const south = panoTileFor(bounds.getSouth(), 0).y
  const tiles: Array<{ x: number; y: number }> = []
  for (let x = west; x <= east; x++) {
    for (let y = north; y <= south; y++) {
      tiles.push({ x: ((x % scale) + scale) % scale, y })
    }
  }
  return tiles
}

function drawPanoDots() {
  if (!map || !panoDotLayer) return
  panoDotLayer.clearLayers()
  if (map.getZoom() < PANO_DOTS_ZOOM || !markersVisible.value) return
  for (const tile of visiblePanoTiles()) {
    for (const dot of panoDotTiles.get(panoTileKey(tile)) || []) {
      panoDotLayer.addLayer(L.circleMarker([dot.lat, dot.lng], {
        pane: 'panoPointsPane',
        renderer: panoRenderer,
        radius: 2.5,
        color: '#ff0000',
        weight: 0,
        fillColor: '#ff0000',
        fillOpacity: 0.7,
        interactive: false,
      }))
    }
  }
}

function refreshPanoDots() {
  if (!map || !panoDotLayer) return
  if (map.getZoom() < PANO_DOTS_ZOOM || !markersVisible.value) {
    panoDotLayer.clearLayers()
    return
  }
  const tiles = visiblePanoTiles()
  const pending = tiles.filter(tile => {
    const key = panoTileKey(tile)
    return !panoDotTiles.has(key) && !panoDotFetching.has(key)
  })
  if (pending.length) {
    pending.forEach(tile => panoDotFetching.add(panoTileKey(tile)))
    void invoke<PanoDotTile[]>('fetch_pano_dots', {
      tiles: pending.map(tile => [tile.x, tile.y]),
    }).then(result => {
      for (const tile of result) panoDotTiles.set(panoTileKey(tile), tile.dots)
      pending.forEach(tile => {
        const key = panoTileKey(tile)
        panoDotFetching.delete(key)
        if (!panoDotTiles.has(key)) panoDotTiles.set(key, [])
      })
      drawPanoDots()
    }).catch(error => {
      console.warn('pano dots failed', error)
      pending.forEach(tile => {
        const key = panoTileKey(tile)
        panoDotFetching.delete(key)
        if (!panoDotTiles.has(key)) panoDotTiles.set(key, [])
      })
      drawPanoDots()
    })
  }
  drawPanoDots()
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

function startElapsedTimer() {
  runElapsed.value = 0
  runStartedAt = performance.now()
  if (elapsedTimer) window.clearInterval(elapsedTimer)
  elapsedTimer = window.setInterval(() => {
    runElapsed.value = (performance.now() - runStartedAt) / 1000
  }, 100)
}

function stopElapsedTimer() {
  if (!elapsedTimer) return
  runElapsed.value = (performance.now() - runStartedAt) / 1000
  window.clearInterval(elapsedTimer)
  elapsedTimer = undefined
}

function formatElapsed(seconds: number) {
  const whole = Math.floor(seconds)
  if (whole < 60) return `${seconds.toFixed(1)}s`
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
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
  runElapsed.value = 0
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
  goalSentToEngine.clear()
  polygonInput.forEach(p => goalSentToEngine.set(p.id, p.nbNeeded))
  try {
    await invoke('start_generation', {
      settings: settings.value,
      polygons: polygonInput,
    })
    startElapsedTimer()
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
  stopElapsedTimer()
  running.value = false
  stopping.value = false
  status.value = 'Stopped'
}

const confirmOpen = ref(false)
const clearAllOpen = ref(false)
const removePolyOpen = ref(false)
const polygonToRemove = ref<Polygon | null>(null)
const savedPolygons = ref<any[]>([])
const savedPolygonQuery = ref('')
const savePolyOpen = ref(false)
const polygonToSave = ref<Polygon | null>(null)
const saveName = ref('')
const saveError = ref('')
const renameOpen = ref(false)
const renameName = ref('')
const renameSessionPolygon = ref<Polygon | null>(null)
const renameSavedPolygonTarget = ref<any>(null)
const renameError = ref('')
const deleteSavedOpen = ref(false)
const savedPolygonToDelete = ref<any>(null)
const exportNotice = ref('')
const copying = ref(false)
let exportNoticeTimer: number | undefined

const filteredSavedPolygons = computed(() => {
  const query = savedPolygonQuery.value.trim().toLowerCase()
  if (!query) return savedPolygons.value
  return savedPolygons.value.filter(item => String(item.name || '').toLowerCase().includes(query))
})

/** "1 location" / "24 locations". */
function locationCount(count: number) {
  return `${count} ${count === 1 ? 'location' : 'locations'}`
}

/** Flag image URL (falls back to the ISO code if it cannot load). */
function flagUrl(code: string) {
  return `https://flagcdn.com/16x12/${code.toLowerCase()}.png`
}

function onFlagError(event: Event) {
  const img = event.target as HTMLImageElement
  if (img) {
    img.style.display = 'none'
    const code = img.nextElementSibling as HTMLElement | null
    if (code) code.style.display = 'inline'
  }
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
  clearAllLocations()
  status.value = 'Generation reset'
}

/** Clears countries, locations and drawings but keeps the settings. */
function clearGenerationState() {
  stopElapsedTimer()
  runElapsed.value = 0
  seenMarkers.clear()
  goalSentToEngine.clear()
  cancelDrawing()
  if (editing.value) cancelEdit()
  markers.value = []
  markerLayers.length = 0
  markerLayer.clearLayers()
  panoDotLayer.clearLayers()
  markersVisible.value = true
  markerLayer.addTo(map)
  panoDotLayer.addTo(map)
  polygons.value = []
  customLayer.clearLayers()
  refreshCountryStyles()
  // Write immediately: the app can be closing right after this.
  persistPolygons(true)
}

function cancelReset() {
  confirmOpen.value = false
}

function makeEditMarker(pt: L.LatLng, idx: number): L.Marker {
  const icon = L.divIcon({ className: 'edit-vertex', html: '', iconSize: [12, 12], iconAnchor: [6, 6] })
  const marker = markRaw(L.marker(pt, { icon, draggable: true, pane: 'markersPane' } as any).addTo(map))
  marker.on('drag', () => {
    const next = [...editPoints.value]
    next[idx] = markRaw(marker.getLatLng())
    editPoints.value = next
    if (editLine.value) editLine.value.setLatLngs(next)
  })
  // Dropping a vertex splits its two neighbouring edges, leaving a new
  // handle on each side so the shape can be refined further (matches the
  // web editor's behaviour).
  marker.on('dragend', () => {
    const pts = [...editPoints.value]
    const n = pts.length
    if (n < 3 || idx >= n) return
    const prevIdx = (idx - 1 + n) % n
    const nextIdx = (idx + 1) % n
    const midPrev = markRaw(L.latLng(
      (pts[prevIdx].lat + pts[idx].lat) / 2,
      (pts[prevIdx].lng + pts[idx].lng) / 2,
    ))
    const midNext = markRaw(L.latLng(
      (pts[idx].lat + pts[nextIdx].lat) / 2,
      (pts[idx].lng + pts[nextIdx].lng) / 2,
    ))
    const updated = [...pts]
    updated.splice(idx + 1, 0, midNext)
    updated.splice(idx, 0, midPrev)
    editPoints.value = updated
    rebuildEditMarkers()
    if (editLine.value) editLine.value.setLatLngs([...editPoints.value])
  })
  marker.on('dblclick', (e: any) => { L.DomEvent.stopPropagation(e); removeEditVertex(idx) })
  return marker
}

function startEdit(polygon: Polygon) {
  if (drawing.value) finishDraw()
  if (editing.value) finishEdit()
  editing.value = polygon
  const coords = (polygon.geometry as any).coordinates[0] as number[][]
  editPoints.value = coords.slice(0, -1).map(c => markRaw(L.latLng(c[1], c[0])))
  // Hide the original polygon layer while editing; other custom polygons stay.
  customLayer.clearLayers()
  polygons.value.filter(p => p.id !== polygon.id && !p.id.startsWith('country-')).forEach(addCustomLayer)
  editMarkers.value = editPoints.value.map((pt, idx) => makeEditMarker(pt, idx))
  editLine.value = markRaw(L.polygon([...editPoints.value], {
    pane: 'customPane', renderer: customRenderer, interactive: false,
    color: '#e07a3f', weight: 2, fillOpacity: 0.1,
  }).addTo(map))
  map.doubleClickZoom.disable()
  map.getContainer().style.cursor = 'grab'
  status.value = 'Drag vertices to edit · double-click a vertex to remove · Enter/Esc or Cancel edit to finish'
}

function removeEditVertex(idx: number) {
  if (editPoints.value.length <= 3) return
  const m = editMarkers.value[idx]
  editMarkers.value = editMarkers.value.filter((_, i) => i !== idx)
  editPoints.value = editPoints.value.filter((_, i) => i !== idx)
  if (m) map.removeLayer(m as any)
  // Rebuild markers so their indices match the new point list.
  rebuildEditMarkers()
  if (editLine.value) editLine.value.setLatLngs([...editPoints.value])
}

function rebuildEditMarkers() {
  editMarkers.value.forEach(m => map.removeLayer(m as any))
  editMarkers.value = editPoints.value.map((pt, idx) => makeEditMarker(pt, idx))
}

function finishEdit() {
  if (!editing.value) return
  const polygon = editing.value
  const coordinates = editPoints.value.map(p => [p.lng, p.lat])
  coordinates.push(coordinates[0])
  polygon.geometry = { type: 'Polygon', coordinates: [coordinates] }
  cancelEdit(false)
  restoreCustomLayers()
  persistPolygons()
  status.value = 'Polygon updated'
}

function cancelEdit(restore = true) {
  editMarkers.value.forEach(m => map.removeLayer(m as any))
  editMarkers.value = []
  if (editLine.value) { map.removeLayer(editLine.value as any); editLine.value = null }
  editPoints.value = []
  editing.value = null
  map.doubleClickZoom.enable()
  map.getContainer().style.cursor = ''
  if (restore) restoreCustomLayers()
  status.value = 'Editing cancelled'
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

function clearAllLocations() {
  seenMarkers.clear()
  goalSentToEngine.clear()
  polygons.value.forEach(polygon => {
    polygon.found.forEach(marker => removeMarkerRecord(marker))
    polygon.found = []
  })
  markers.value = []
  markerLayers.length = 0
  markerLayer.clearLayers()
  persistPolygons()
}

function confirmClearAllLocations() {
  clearAllOpen.value = false
  clearAllLocations()
  status.value = 'All locations cleared'
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
  if (copying.value) return
  copying.value = true
  // Let the spinner paint before the (possibly heavy) serialisation blocks.
  await new Promise<void>(resolve => requestAnimationFrame(() => resolve()))
  try {
    const text = JSON.stringify(buildExport(markerList))
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      const area = document.createElement('textarea')
      area.value = text
      document.body.appendChild(area)
      area.select()
      document.execCommand('copy')
      document.body.removeChild(area)
    }
    showExportNotice(`Copied ${locationCount(markerList.length)}`)
  } catch {
    showExportNotice('Could not copy to the clipboard')
  } finally {
    copying.value = false
  }
}

// Computed from polygons only (O(polygons)), so pushing thousands of markers
// does not rebuild the name on every location event.
const exportFileName = computed(() => {
  const countries = [...new Set(polygons.value.map(polygon => (polygon.name || '').trim()).filter(Boolean))]
  const label = countries.length === 0
    ? 'All-countries'
    : countries.length === 1
      ? countries[0]
      : `${countries.length}-countries`
  return `${safeName(label)}-${markers.value.length}-locations`
})

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
  if (event.code === 'Escape') {
    if (drawing.value) {
      cancelDrawing()
      status.value = 'Drawing cancelled'
    } else if (editing.value) {
      cancelEdit()
    }
  }
  if (event.code === 'Enter' && editing.value) {
    event.preventDefault()
    finishEdit()
  }
  if (drawing.value && (event.code === 'Backspace' || event.code === 'Delete')) {
    event.preventDefault()
    removeLastDrawPoint()
  }
}

async function onNotificationToggle() {
  const notification = settings.value.notification
  if (!notification.enabled || notification.target === 'discord') return
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
