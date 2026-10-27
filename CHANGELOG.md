# Changelog

Alle noemenswaardige wijzigingen in DAW 388. Versienummers volgen [Semantic Versioning](https://semver.org/lang/nl/).

## [0.5.0] - 2026-10-27

### Toegevoegd
- Audio-instellingen in het instellingenvenster: uitvoerapparaat, samplerate (44,1 tot 192 kHz, voor zover het apparaat die biedt) en buffergrootte (32 tot 2048 samples), met de latency in milliseconden.
- De app onthoudt de gekozen uitvoer tussen het opstarten (`settings.json` in de configuratiemap).
- Valt het apparaat weg, dan schakelt de app meteen over op het standaardapparaat, op dezelfde plek, en terug zodra het gekozen apparaat er weer is.
- Speelde de band, dan speelt hij na het omschakelen van apparaat, samplerate of buffer gewoon door.
- Stille klok: zonder enige audio-uitvoer blijven transport, teller en meters gewoon werken.

### Gewijzigd
- Een gekozen apparaat dat ontbreekt geeft nu een duidelijke melding ("... niet gevonden, nu via ...") in plaats van stil het standaardapparaat te gebruiken.

## [0.4.0] - 2026-10-27

### Toegevoegd
- Transport naar het voorbeeld van de 388: RESET (nulpunt), CUE (geheugenpunt), STC (naar het CUE-punt), REPEAT (herhalen van nulpunt tot CUE-punt), REW en F FWD (stil spoelen op 30 keer de snelheid), STOP, PLAY en RTZ (naar het nulpunt).
- Teller vanaf het nulpunt, met een minteken ervoor, en het CUE-punt eronder.
- PITCH-knop: snelheid en toonhoogte plus of min 15%, met VARI/FIX-schakelaar. Dubbelklik zet hem op 0%.
- Instellingenvenster (Bestand, Instellingen of Cmd+,) met de REPEAT-modus: 388 (de band spoelt in de tijd terug, zoals het origineel) of cycle (springt direct terug, zoals in Logic).
- Nulpunt, CUE-punt, REPEAT-modus, pitch en VARI/FIX worden in het project opgeslagen.
- Sneltoetsen: komma is REW, punt is F FWD, C zet REPEAT aan of uit.
- Bestanden en projecten laden op de achtergrond; het spoor toont "laden..." tot het klaar is.

### Gewijzigd
- RTZ gaat naar het nulpunt (RESET) in plaats van altijd naar het begin. In de 388-modus spoelt RTZ in de tijd.

## [0.3.0] - 2026-10-27

### Toegevoegd
- Schakelaar TRACK METERS PRE/POST in de master-sectie. PRE (standaard, zoals op de 388) toont het niveau van het spoor zelf, POST het niveau na fader en mute. De stand wordt in het project opgeslagen; oudere projecten openen met PRE.

### Gewijzigd
- Fader-curve naar het voorbeeld van Logic Pro: +6 dB bovenaan, 0 dB op ongeveer driekwart en rond 0 dB ongeveer 11 punten per dB (met Shift 55). Schaal 6, 3, 0, -3, -6, -10, -15, -20, -30, -40 en ∞. Een fader die in een eerder project hoger dan +6 dB stond, opent op +6 dB.
- Meters staan rechts van de fader en hebben een eigen schaal zoals in Logic: 0 dBFS bovenaan, gelijke stappen tot -24 en daaronder samengedrukt tot -60.

## [0.2.2] - 2026-10-27

### Gewijzigd
- Faders hebben een mengtafel-curve: 0 dB zit op ongeveer driekwart van de baan en tussen -10 en +5 dB is veel ruimte (ongeveer 6 punten per dB, met Shift 30). De stille kant onderin is samengedrukt.
- Langere faderbaan (260 punten) met schaalverdeling (+10 tot -40 en ∞) naast elke fader.
- Een dichte fader toont "-∞ dB".

## [0.2.1] - 2026-10-27

### Toegevoegd
- Import van AIFF en AIFF-C (`.aif`, `.aiff`, `.aifc`): 8, 16, 24 en 32-bit integer en 32-bit float, mono en stereo, elke samplerate.
- Het formaat wordt herkend aan de inhoud van het bestand, dus een verkeerde extensie is geen probleem.

### Gewijzigd
- De bestandskiezer toont WAV- en AIFF-bestanden; de menuknop heet nu "8 audiobestanden laden".

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
