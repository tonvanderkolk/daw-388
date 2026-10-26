# Changelog

Alle noemenswaardige wijzigingen in DAW 388. Versienummers volgen [Semantic Versioning](https://semver.org/lang/nl/).

## [0.2.0] - 2026-10-26

### Toegevoegd
- Projecten opslaan en openen: een projectmap met `project.json` (fader, pan en mute per spoor, master) en een submap `audio/`. Bij het opslaan worden de WAV-bestanden naar `audio/` gekopieerd, zodat de projectmap op zichzelf staat.
- Menu Bestand met Nieuw, Openen, Opslaan en Opslaan als, plus de sneltoetsen Cmd+N, Cmd+O, Cmd+S en Shift+Cmd+S (Ctrl op Windows).
- Autosave 30 seconden na een wijziging, zodra het project een map heeft. Bovenin staat "Opgeslagen" of "Automatisch opgeslagen" met de tijd.
- Backups van `project.json` in `backups/`: bij elke handmatige opslag en hooguit elke 5 minuten bij autosave. De laatste 10 blijven bewaard.
- Vraag om op te slaan bij Nieuw, Openen, Project sluiten, Cmd+Q of het sluiten van het venster als er niet-opgeslagen wijzigingen zijn.
- Project sluiten (Cmd+W) en Stop DAW 388 (Cmd+Q) in het menu Bestand. Op macOS liep Cmd+Q eerst buiten de opslaan-vraag om; het standaard app-menu van macOS is daarom uitgezet.
- Projectnaam in de titelbalk, met een sterretje bij niet-opgeslagen wijzigingen.
- Ontbrekende audiobestanden worden per spoor in rood gemeld en blijven in het project staan, zodat het terugzetten van het bestand het project herstelt.
- Een projectmap of `project.json` kan als argument worden meegegeven.

### Gewijzigd
- CI: tests draaien in het release-profiel, zodat de release-build dezelfde compilatie hergebruikt. Dat scheelt vooral op Windows veel tijd.
- CI: `actions/checkout` en `actions/upload-artifact` bijgewerkt naar v7 (Node.js 24), wat de waarschuwing over Node.js 20 oplost.

## [0.1.1] - 2026-10-26

### Opgelost
- Tikjes bij het verschuiven van de stereo master-fader: de master-gain verandert nu geleidelijk, net als de spoorfaders.
- Een dichtgetrokken fader geeft nu echt stilte.

## [0.1.0] - 2026-10-26

### Toegevoegd
- Cargo workspace met de crates engine, tape, fx, project, ui en app-desktop.
- Audio-engine met 8 mono sporen, transport (PLAY, STOP, RTZ, locate) en een stereomix met gain, constant-power pan, mute en master, plus piekmeters per spoor en voor de stereo-uitgang.
- Lock-free communicatie tussen UI en audiothread; buffers worden alleen op de UI-thread vrijgegeven.
- WAV-import (8 tot en met 32-bit integer en 32-bit float), mixdown naar mono en samplerate-conversie naar het apparaat.
- Desktop-app (macOS en Windows) met keuze van het audio-uitvoerapparaat en terugval op het standaardapparaat.
- Venster met 8 kanaalstrips (laden, wissen, pan, mute, fader, meter), stereo master, LED-teller en transporttoetsen. Sneltoetsen spatie en Home.
- WAV-bestanden kunnen als argument worden meegegeven.
- GitHub Actions-workflow die test en bouwt voor macOS en Windows.
- PROJECT.md, ROADMAP.md, README.md en licenties (MIT OR Apache-2.0).
