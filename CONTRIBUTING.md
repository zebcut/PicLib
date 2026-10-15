# Contributing

Thanks for wanting to help. A few things to know before you dive in.

## Getting started

See the README for the build setup, then:

```bash
npm install
npm run app
```

Run `npm run check` before opening a PR; it must pass with zero warnings.

## How the code is laid out

```
src-tauri/src/       Rust core (Tauri 2)
  domain/            plain business types, no I/O
  catalog/           SQLite access, migrations, backups
  files/             every disk operation (copy, move, trash, sidecars)
  import/ metadata/ thumbnails/ keywords/ stacks/ ...  core services
  commands/          thin Tauri commands, no business logic
  platform/          everything OS-specific (macOS today, Windows next)
src/                 React 19 + TypeScript (strict) UI
  views/ components/ state/ services/ menu/ shortcuts/ i18n/ theme/
scripts/             place database builder (GeoNames)
```

Some ground rules:

- **User files are sacred.** Read the golden rules in the README. Any disk
  write goes through `files/`, never overwrites, never deletes for good.
- **Platform-specific code lives in `platform/`** only. That is where the
  Windows port happens (thumbnails via WIC, trash, file watching, external
  editors, disks…). Help there is very welcome.
- **The TypeScript bindings are generated** from the Rust commands
  (`tauri-specta`). Never edit `src/services/bindings.ts` by hand: it is
  regenerated every time you start `npm run app`.
- **Schema changes** are new numbered SQL migrations in
  `src-tauri/src/catalog/migrations/`. Never edit a published one.
- **No hardcoded UI strings**: everything goes through `src/i18n/locales`
  (English and French).
- **Shortcuts** are declared in the central registry (`src/shortcuts/`),
  never as ad-hoc key listeners.
- Rust: typed errors (`thiserror`), no `unwrap()` / `expect()` in production
  code, Clippy pedantic.

A good chunk of the code comments are in French (the project started that
way). Translating them as you go is a perfectly good first contribution.

## Pull requests

- One topic per PR, small commits, [Conventional Commits](https://www.conventionalcommits.org)
  style messages.
- Sign off your commits (`git commit -s`): by doing so you certify the
  [Developer Certificate of Origin](https://developercertificate.org).
- Don't copy code, icons or UI text from proprietary software. Reproducing a
  feature is fine, reusing their assets is not.
- New dependencies must have a GPL-3.0-compatible license and should be
  mentioned in `THIRD_PARTY_NOTICES.md`.
- Never commit personal photos. If you need sample files, make them yourself
  and strip GPS, names and faces.

## Code of conduct

Be nice. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
