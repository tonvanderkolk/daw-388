# Changelog

Alle noemenswaardige wijzigingen in DAW 388. Versienummers volgen [Semantic Versioning](https://semver.org/lang/nl/).

## [Unreleased]

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
