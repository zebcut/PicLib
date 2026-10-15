# PicLib

A fast, local-first photo library manager for the desktop. Think "the Library
module of a pro photo app", without the subscription, without the cloud, and
without ever touching your original files.

PicLib is free software (GPL-3.0-or-later). It is **macOS-only for now**; a
Windows version is planned (the core is cross-platform, the platform layer
still needs its Windows half).

> Status: early days (0.1). Expect rough edges and the odd catalog migration
> (the catalog is backed up first). Your photos are safe either way: PicLib
> never writes into them.

## What it does

- **Catalog** — import by reference or by verified copy (size + SHA-256),
  from folders or memory cards, with date-based folder layouts, renaming
  templates, duplicate detection and an optional backup copy.
- **Fast browsing** — virtualized grid that stays smooth at 50k+ items,
  Loupe with 100 % zoom, Compare, Survey and a fullscreen Cull mode. RAW,
  HEIC, JPEG, PNG, TIFF, PSD and the usual video formats.
- **Triage** — flags, stars, color labels, rotation, with keyboard shortcuts
  you probably already know. Everything is undoable (Cmd+Z).
- **Metadata** — title, caption, creator, location fields, capture time
  editing, metadata templates, reading existing XMP / IPTC / DxO `.dop`.
- **Keywords** — hierarchical keywords, keyword sets, suggestions,
  drag & drop, text import/export.
- **Organize** — stacks (manual, RAW + JPEG pairs, bursts), manual and smart
  collections, quick collection, a filter bar with presets.
- **Map** — OpenStreetMap view, drag photos onto the map to geotag them,
  offline reverse geocoding (GeoNames), filter the grid by the visible area.
- **Files** — move, rename, delete (to the Trash, always), relocate missing
  files, resync folders, operation log.
- **XMP sidecars** — writes your metadata to `.xmp` files (merging, never
  clobbering what other apps wrote) and notices when another app changed them.
- **External editors** — "Edit in" DxO PhotoLab, darktable, Affinity Photo,
  Capture One, Luminar, Photoshop, Lightroom Classic or any app; exported
  results come back on their own and get stacked with their original.

What it deliberately does **not** do: develop/retouch, export, cloud sync, face
recognition.

## The golden rules

Whatever you change, these hold:

1. Original files are never modified (metadata goes to the catalog and to XMP
   sidecars).
2. Nothing is ever deleted for good: deleting means the system Trash, after a
   confirmation.
3. Nothing is ever written to an import source (memory card, camera).
4. Every copy is verified before it counts.
5. An existing file is never overwritten (`-2`, `-3`… suffixes).
6. Every disk operation goes through the `files` module.
7. Bulk operations show a summary and ask first.
8. The catalog is backed up automatically, and before every migration.

## Download

Grab the `.dmg` from the [Releases](../../releases) page.

The app is not signed with an Apple Developer ID, so macOS will complain the
first time. Right-click `PicLib.app` → **Open** → **Open**, or run:

```bash
xattr -dr com.apple.quarantine /Applications/PicLib.app
```

Requires macOS 13 (Ventura) or later.

## Building from source

You need Xcode command line tools, [rustup](https://rustup.rs) and Node.js 24.

```bash
xcode-select --install
brew install rustup node@24
npm install
npm run app        # dev build with hot reload
npm run dmg        # release build + .dmg
```

The Rust version is pinned in `rust-toolchain.toml`; rustup fetches it on the
first build. The Rust build produces several GB: set `CARGO_TARGET_DIR` if you
want it outside the repo, and keep it off synced folders (iCloud, OneDrive)
and off ExFAT disks.

Useful scripts:

| Command          | What it does                                     |
| ---------------- | ------------------------------------------------ |
| `npm run app`    | Run in development mode                          |
| `npm run check`  | Format check, type check, ESLint, Clippy         |
| `npm run format` | Format everything (Prettier + rustfmt)           |
| `npm run dmg`    | Release build and disk image                     |
| `npm run places` | Rebuild the offline place database from GeoNames |

## Where things live

| What     | Where                                                              |
| -------- | ------------------------------------------------------------------ |
| Catalog  | `~/Documents/PicLib.nosync/` by default (catalog, backups, caches) |
| Settings | `~/Library/Application Support/com.piclib.desktop/settings.json`   |
| Logs     | `~/Library/Logs/com.piclib.desktop/` (`PICLIB_LOG=debug` for more) |

A new install starts with an empty catalog. There is no sample data.

## Contributing

PRs and issues are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Security
issues: [SECURITY.md](SECURITY.md).

## License

[GPL-3.0-or-later](LICENSE). Third-party components and data:
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

PicLib is an independent project, not affiliated with or endorsed by Adobe,
Apple, DxO or any other vendor mentioned here.
