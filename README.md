# Map Generator

A fast, low-memory desktop app for generating street-level panorama locations inside GeoJSON polygons.

This project was hugely inspired by [various-map-generator](https://various-map-generator.pages.dev/). This is a native desktop take on it: offline-friendly, with built-in country polygons and automatic updates. Repository:

## What it does

- Pick one or more countries or territories (or draw your own polygon / import GeoJSON) on the map.
- Generate random panorama locations inside them using Google Street View and 13 other providers.
- Filter by camera generation, blue lines, road type, date, altitude, links, description, author and more.
- Export the results as JSON or CSV, or copy them to the clipboard.
- Blue-line overlays show where street view coverage exists.
- Click any generated pin to open its panorama in the browser.

## Supported providers

Google, Apple, Yandex, Bing, Naver, Kakao, Baidu, Tencent, Mapillary, Mapy.cz, ASIG, Já360, Vegbilder, OpenMap.

## Download and install

Grab the latest installer from the [Releases page](https://github.com/hannogeo/map-generator/releases). The app checks for updates automatically and will offer to install new versions when available.

## How to use it

1. **Select areas.** Click countries on the map, search for them in the list, or draw/import your own polygon.
2. **Set a goal.** Each selected area shows how many locations you want from it (default 1000).
3. **Choose a provider and filters.** Pick the street-view provider and tune the filters (camera generation, dates, blue lines, etc.).
4. **Generate.** Click *Start generation*. Pins appear on the map as locations are found. Click a pin to open its panorama.
5. **Export or reset.** Use *Export* to save your locations (JSON/CSV/copy), or *Reset* to clear everything and start over (settings are kept).

## Settings

Everything is saved automatically and persists between sessions: provider, radius, strategy (random/grid), number of generators, speed, filters, notification preferences and more. Use *Reset* to clear locations and selected countries while keeping your settings.

## Notifications

Optional desktop (and Discord webhook) notifications when a location is found, a polygon is complete, or all polygons are complete.

## Performance

Generation runs in the Rust backend with parallel sampling, so it's **up to ~2× faster** than the browser-based version with the same settings, while using noticeably less RAM. The UI is a thin webview (Tauri), so the app stays lightweight.

## Credits

- Inspired by and largely derived from [various-map-generator](https://various-map-generator.pages.dev/).
- Country borders from public-domain world atlas data.