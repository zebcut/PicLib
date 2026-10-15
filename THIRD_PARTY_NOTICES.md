# Third-party notices

PicLib is distributed under GPL-3.0-or-later (see [LICENSE](LICENSE)). It
includes or uses the following components and data, under their own licenses.

## Bundled data

**GeoNames** — `src-tauri/data/places/` (`places.tsv`, `regions.tsv`), built
by `npm run places` from [GeoNames](https://www.geonames.org)
(`cities500.txt`, `admin1CodesASCII.txt`), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Changes: city
districts and abandoned, historical or destroyed places removed, coordinates
rounded to 4 decimals, unused columns dropped. Also credited in the About
window.

**Unicode CLDR** — country names in `src-tauri/data/places/countries.tsv`,
under the [Unicode License](https://www.unicode.org/license.txt).

## Downloaded at runtime

**OpenStreetMap** map tiles, downloaded on demand and cached locally. Data ©
OpenStreetMap contributors, [ODbL](https://www.openstreetmap.org/copyright);
tiles served by the OpenStreetMap Foundation under its
[tile usage policy](https://operations.osmfoundation.org/policies/tiles/).
Attribution is shown on the map.

## Libraries

Notably [Tauri](https://tauri.app), [React](https://react.dev),
[SQLite](https://www.sqlite.org) (public domain), `rusqlite`, `nom-exif`,
`quick-xml`, `jiff`, `sha2`, `walkdir`, `specta`, `ureq`, `notify` (CC0),
`i18next`, [Leaflet](https://leafletjs.com) (BSD-2-Clause) and their
dependencies, each under its own license. On macOS, PicLib uses the system
frameworks (ImageIO, AVFoundation, Core Image, Metal).

Full lists:

```bash
cargo install cargo-license && (cd src-tauri && cargo license)
npx license-checker --production --summary
```

## Trademarks

All product names mentioned (Adobe, Lightroom, Photoshop, DxO PhotoLab,
Capture One, Affinity Photo, Luminar, darktable…) are trademarks of their
respective owners and are only used to describe interoperability.

## DNG

This product includes DNG technology under license by Adobe.

PicLib reads DNG files under Adobe's royalty-free
[DNG patent license](https://helpx.adobe.com/camera-raw/desktop/dng-and-file-formats/digital-negative.html).
