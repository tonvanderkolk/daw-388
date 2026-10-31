# CLAUDE.md - DAW 388

> Het projectgeheugen voor Claude. Lees het aan het begin van elke sessie, samen met PROJECT.md.

## Projectbeschrijving
Een eenvoudige standalone 8-sporen DAW naar het model van de Tascam 388 (Studio 8): opnemen, mixen op een 8:8:2-mixer met tape-karakter en exporteren. Een hobbyproject van Ton van der Kolk, gratis en open source, met een donatielink naar Buy Me a Coffee (`tvdk71`). De scope staat in HANDOVER.md, de techniek in PROJECT.md.

## Technische stack
- Rust (stable, edition 2024), Cargo workspace: `engine`, `tape`, `fx`, `project`, `ui`, `app-desktop`
- egui / eframe 0.37, cpal 0.18 (CoreAudio, WASAPI), rtrb, hound, symphonia, rubato
- Builds en releases: GitHub Actions (`.github/workflows/build.yml`), verpakken in `packaging/`
- Cargo staat niet op het PATH: `export PATH="$(brew --prefix rustup)/bin:$PATH"`

## Mappenstructuur
- `crates/`: de workspace (de UI met de skin in `crates/ui/src/skin/`)
- `packaging/`: het icoon, de macOS-bundel (`macos/bundle.sh`, `Info.plist`), het Windows-icoon en `manual.sh` (de handleiding als PDF)
- `docs/manual/`: de gebruikershandleiding (Engels, HTML); bij elke nieuwe functie bijwerken
- `docs/superpowers/`: specs en plannen per versie
- `design/ref/` en de oude mockups in `design/`: handleiding, foto's en ontwerpen, niet in de repo (alleen het routeringsschema staat erin)
- `logs/`: synclogs

## Conventies en afspraken
- De UI is Engels; code en commentaar Engels; documentatie (CHANGELOG, ROADMAP, PROJECT.md) Nederlands
- Geen komma voor en, of, noch; geen em dash of en dash
- Voor elke commit: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- Ton test zelf in de app; committen en pushen alleen na zijn "commit en push"
- Elke versie: CHANGELOG, ROADMAP, PROJECT.md en de versie in Cargo.toml bijwerken
- De repository is openbaar: geen persoonlijke gegevens in bestanden of logs, commits met het noreply-adres van GitHub
- Een release maak je met een label `v*`; dat bouwt de DMG en de ZIP en zet ze in een GitHub Release

## Belangrijke beslissingen
- Twee weergaven, MIXER en TRACKS (Tab wisselt): de mixer staat altijd in zijn geheel in beeld. Voor lage schermen is er de compacte console (View), zonder EQ, AUX en effecten
- De console staat op een vast raster met banden, zoals de echte 388
- Opnemen op VARI mag; de snelheid ligt vast per opname
- Een project is een pakket `Naam.daw388`; oudere mappen blijven werken. Ongebruikte opnames gaan bij het verlaten naar de prullenbak, nooit direct weg
- Geen productnamen van TASCAM als eigen naam (dus 8-TRACK, niet STUDIO 8); de 388 noemen als voorbeeld mag, met de disclaimer
- Niet ondertekend: geen Apple Developer-account. De installatie-uitleg vangt dat op
- Fase 9 (browserversie) is uitgesteld

## Huidige status
Versie 0.30.0: M, S en R per spoor in TRACKS, ongebruikte opnames gaan bij het verlaten van een project naar de prullenbak, een donatielink (Help, Support DAW 388...) en het naambord DAW 388 / 8-TRACK in plaats van STUDIO 8. Sinds 0.29.0 is er een gebruikershandleiding als PDF. Release v0.30.0 staat op GitHub met de DMG, de ZIP en de handleiding.

## Actieve branch
main

## Laatste sync
31-10-2026 13:14

## Bekende issues / openstaande punten
- Nog niet getest op een andere Mac en op Windows
- Een project openen met een dubbelklik in de Finder werkt nog niet (winit geeft het open-event niet door)
- Windows: geen ASIO, geen eigen ARM-build
