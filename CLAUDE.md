# CLAUDE.md - DAW 388

> Het projectgeheugen voor Claude. Lees het aan het begin van elke sessie, samen met PROJECT.md.

## Projectbeschrijving
Een eenvoudige standalone 8-sporen DAW naar het model van de Tascam 388 (Studio 8): opnemen, mixen op een 8:8:2-mixer met tape-karakter en exporteren. Een hobbyproject van Ton van der Kolk, gratis te delen, niet te verkopen. De scope staat in HANDOVER.md, de techniek in PROJECT.md.

## Technische stack
- Rust (stable, edition 2024), Cargo workspace: `engine`, `tape`, `fx`, `project`, `ui`, `app-desktop`
- egui / eframe 0.37, cpal 0.18 (CoreAudio, WASAPI), rtrb, hound, symphonia, rubato
- Builds en releases: GitHub Actions (`.github/workflows/build.yml`), verpakken in `packaging/`
- Cargo staat niet op het PATH: `export PATH="$(brew --prefix rustup)/bin:$PATH"`

## Mappenstructuur
- `crates/`: de workspace (de UI met de skin in `crates/ui/src/skin/`)
- `packaging/`: het icoon, de macOS-bundel (`macos/bundle.sh`, `Info.plist`) en het Windows-icoon
- `docs/superpowers/`: specs en plannen per versie
- `design/ref/`: handleiding en foto's van de 388, niet in de repo
- `logs/`: synclogs

## Conventies en afspraken
- De UI is Engels; code en commentaar Engels; documentatie (CHANGELOG, ROADMAP, PROJECT.md) Nederlands
- Geen komma voor en, of, noch; geen em dash of en dash
- Voor elke commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- Ton test zelf in de app; committen en pushen alleen na zijn "commit en push"
- Elke versie: CHANGELOG, ROADMAP, PROJECT.md en de versie in Cargo.toml bijwerken
- Een release maak je met een label `v*`; dat bouwt de DMG en de ZIP en zet ze in een GitHub Release

## Belangrijke beslissingen
- Twee weergaven, MIXER en TRACKS (Tab wisselt): de mixer staat altijd in zijn geheel in beeld
- De console staat op een vast raster met banden, zoals de echte 388
- Opnemen op VARI mag; de snelheid ligt vast per opname
- Een project is een pakket `Naam.daw388`; oudere mappen blijven werken
- Niet ondertekend: geen Apple Developer-account. De installatie-uitleg vangt dat op
- Fase 9 (browserversie) is uitgesteld

## Huidige status
Versie 0.25.1: de eerste release (v0.25.0) staat op GitHub met een universal DMG en een Windows-ZIP; de installatie-uitleg voor macOS is aangescherpt.

## Actieve branch
main

## Laatste sync
30-10-2026 00:12

## Bekende issues / openstaande punten
- De repository is privé; Ton maakt hem openbaar na verder testen
- Nog niet getest op een andere Mac en op Windows
- Een project openen met een dubbelklik in de Finder werkt nog niet (winit geeft het open-event niet door)
- Windows: geen ASIO, geen eigen ARM-build
