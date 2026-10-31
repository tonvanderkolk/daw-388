# HANDOVER - DAW 388

Gegenereerd: 26-10-2026
Versie: 0.1.0 (concept)
Status: Nieuw project (hobby)

---

## Wat is dit project?

DAW 388 is een super eenvoudige standalone 8-sporen DAW, qua opzet en specs gebaseerd op de legendarische Tascam 388 (Studio 8): een 8-kanaals mixer gecombineerd met een 8-sporen bandrecorder. Je neemt op via een audio-interface (of de ingebouwde audio), mixt op een 8:8:2-mixer met tape-karakter en exporteert een stereo bounce of losse sporen om elders verder te mixen. Het is een hobbyproject van Ton van der Kolk dat gratis gedeeld mag worden maar niet verkocht wordt.

---

## Tech stack

- **Taal:** Rust (recente stable)
- **Audio I/O:** `cpal` (CoreAudio op macOS, WASAPI en ASIO op Windows). Gebruikt de gekozen audio-interface als die er is, anders de ingebouwde audio.
- **UI:** `egui` / `eframe`, dezelfde code voor desktop en web
- **Audiobestanden:** `hound` (WAV schrijven/lezen), eventueel `symphonia` voor import van andere formaten
- **Realtime-communicatie:** lock-free ringbuffer (bijvoorbeeld `rtrb`) tussen UI en audiothread
- **Platforms:** macOS en Windows (desktop) als hoofdproduct; browser via WebAssembly in een latere fase
- **Builds:** GitHub Actions voor macOS en Windows (ook Windows bouwen vanaf de Mac)
- **Versiebeheer:** GitHub, repo `tonvanderkolk/daw-388` (eerst privé, sinds 31-10-2026 openbaar)
- **Licentie:** MIT OR Apache-2.0
- **Projectopslag:** projectmap met `project.json` plus een submap `audio/` met WAV-bestanden (werkt gelijk op desktop en web; geen SQLite nodig)

---

## Genomen besluiten

### Platform en architectuur
- Rust gekozen boven openDAW (TypeScript, AGPL) en JUCE (C++): één codebase voor Mac, Windows en later browser.
- Eerst de desktopversie (Mac en Windows) werkend krijgen. De code wordt vanaf dag één zo opgezet dat een WASM/browserversie later zonder ombouw kan volgen (audio-engine los van platformspecifieke code).
- De browserversie wordt een "lite"-versie: opnemen heeft daar meer latency en vraagt cross-origin isolation headers.
- Geen plug-in-ondersteuning (geen AU/VST). Wie verder wil mixen exporteert de sporen.

### SoundCraft als referentie
- SoundCraft (https://github.com/storytold/soundcraft) is een Rust/egui-reimplementatie van Pro Tools onder MIT OR Apache-2.0 (pre-alpha).
- Claude Code beoordeelt zelf wat bruikbaar is: overnemen mag (licentie staat het toe) mits het goed werkt; behoud dan copyright- en licentievermeldingen. Anders dient het als inspiratie.

### Functionele scope
- **8 sporen**, 1 opname/weergave per spoor.
- **Opnemen** via audio-interface of ingebouwde audio, met REC FUNCTION per spoor (arm-knoppen met lampje), punch in/out en metronoom.
- **Transport:** RESET, RTZ, CUE, REPEAT, REW, F FWD, STOP, PLAY, REC; teller in min:sec; pitch/varispeed-knop.
- **Mixer 8:8:2** per kanaal:
  - Ingangskeuze MIC / LINE / TAPE (TAPE = weergave van het spoor, de "remix"-stand)
  - Trim
  - 3-band sweepable EQ (HIGH, MID, LOW), elk als concentrische knop: binnenknop = gain, buitenring = frequentie (bereik ongeveer 50 Hz tot 15 kHz)
  - AUX-send (groen) met PRE/POST-schakelaar:
    - PRE: cue-/koptelefoonmix voor de muzikant, onafhankelijk van de kanaalfader. Gaat naar een aparte uitgang van de audio-interface als die er is.
    - POST: tweede effect-send, standaard naar de plate reverb.
  - EFF-send (blauw), vast post-fader, naar de tape echo. Trek je de fader dicht, dan zakt het effect evenredig mee.
  - ASSIGN-knoppen 1-2, 3-4, 5-6, 7-8 en L-R (busrouting, nodig voor ping-pong bouncen)
  - Pan en fader
- **Master-sectie:** EFFECT RTN (gain en pan voor plate en echo), MONITOR (gain en pan), AUX- en EFF-masters, STEREO MASTER-fader.
- **Effecten:** twee ingebouwde effecten passend bij het tijdperk: plate reverb (aux) en tape echo (eff).
- **Bewerken:** niet-destructieve regio's die je kunt verschuiven, knippen, kopiëren, plakken en splitsen ("splice").
- **Undo:** kort, maximaal 10 stappen.
- **Ping-pong bounce:** sporen via de assign-bussen samenvoegen naar een vrij spoor.
- **Export:**
  - Stereo bounce
  - Losse sporen (stems), per spoor aan/uit
  - Verwerkingsopties: met mixer (EQ, pan, fader) en tape-karakter / alleen tape-karakter / helemaal droog
  - Optie "alle sporen beginnen op 0:00"
  - Formaat WAV of AIFF, 16/24/32-bit float, 44,1 / 48 / 96 kHz (standaard 24-bit 48 kHz)
  - Bestandsnaam: `[Projectnaam]_01_Kick.wav`
- **Import** van audiobestanden naar een spoor.

### Tape-emulatie
- De 388 zelf is het enige machineprofiel. Een tweede profiel (bijvoorbeeld Studer A80- of Ampex-stijl) kan later.
- **Bandsnelheden:** 3¾, 7½ (standaard, origineel) en 15 IPS. Snelheid beïnvloedt hoge-tonenrespons, head bump in het laag, ruis en wow/flutter.
- **Tapesoorten:** drie profielen, 456-stijl, 250-stijl en 911-stijl. Het precieze karakter per soort wordt bepaald tijdens het bouwen van het geluidsmodel (benadering op basis van gepubliceerde specs, geen exacte kopie).
- **Regelbaar:** saturatie, wow en flutter, ruis (hiss), crosstalk, varispeed.
- **dbx:** aan/uit per groep (sporen 1-4 en 5-8), optioneel hoorbaar "ademen".
- **Schakelaar Clean / 388** per project.
- **Technieken:** hysterese voor saturatie, gap loss en head bump als filters, wow en flutter als gemoduleerde vertraging, ruis, crosstalk, optioneel dbx-compansie.
- Let op: ChowTapeModel is GPL en dus onverenigbaar met de MIT/Apache-licentie. Alleen als inspiratie gebruiken, geen code overnemen.

### Referentiespecs Tascam 388
- 8:8:2 mixer, 60 mm faders, 3-band sweepable EQ (50 Hz tot 15 kHz), 2 aux-sends per kanaal
- 1/4 inch tape, 7½ ips, 2 koppen, ingebouwde dbx
- Frequentiebereik ongeveer 35 Hz tot 15 kHz (±3 dB), wow en flutter 0,05% WRMS, S/N 90 dB met dbx, crosstalk 75 dB

### Ontwerp (UI)
- Gekozen richting: **variant A2 "Vintage, dichter bij de 388"** (skeuomorf, lijkt op het apparaat maar moet vooral goed te bedienen zijn).
- Bovenaan een meterbrug: 8 kleine VU-meters (TRK 1-8) en 2 grotere (STEREO L / AUX en STEREO R / EFFECT), naamplaat "DAW 388 / STUDIO 8" rechts.
- Mixer links, bandrecorder rechts (rookglazen venster met spoelen, rode LED-teller, grijze transporttoetsen, pitchknop, snelheidskeuze, BOUNCE- en EXPORT-knop).
- Knopkleuren per functie: trim rood, EQ oranje met grijze buitenring, aux groen, eff blauw, pan crème. Fadercaps crème, aux/eff-masters oranje, stereo master rood. REC FUNCTION-knoppen rood.
- Donkergrijs paneel in een bordeauxbruine houten omlijsting met leren armsteun.
- Daaronder een bewerkingspaneel met de 8 sporen (regio's, tijdlijn, afspeelpositie) en een werkbalk in dezelfde stijl.
- Geen Tascam-logo of exacte kopie van de vormgeving gebruiken; eigen naamplaat zodat het zonder merkproblemen gedeeld kan worden.
- Tape-instellingen en export als aparte vensters (zie mockup "Tape-instellingen en export").

---

## Wat er al bestaat

- **Mockups** op een Claude Design-canvas: "DAW 388 mockups" (https://claude.ai/artifact/DzGNj5KtuHt7Ld2u14BCTi). Relevant: A2 "Vintage, dichter bij de 388" en "Tape-instellingen en export". Varianten A en B zijn verworpen.
- **`mockup-A2-studio8.dc.html`**: de broncode van het A2-ontwerp, meegeleverd als visuele referentie voor indeling, kleuren en labels (niet zelfstandig draaibaar).
- Nog geen code of repo.

### Voorgestelde structuur (Cargo workspace)
```
daw-388/
├── Cargo.toml              # workspace
├── PROJECT.md
├── ROADMAP.md
├── CHANGELOG.md
├── HANDOVER.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── .github/workflows/      # builds macOS + Windows (later WASM)
├── crates/
│   ├── engine/             # transport, sporen, mixer, routing, realtime-audio (platformonafhankelijk)
│   ├── tape/               # tape-emulatie en dbx
│   ├── fx/                 # plate reverb en tape echo
│   ├── project/            # project.json, import/export, autosave, backups
│   ├── ui/                 # egui-interface (vintage skin)
│   └── app-desktop/        # cpal-koppeling en desktop-entrypoint
└── design/
    └── mockup-A2-studio8.dc.html
```

---

## Openstaande vragen

- **Signaalroute verifiëren met de originele Operation & Maintenance Manual** (blokschema). Ton legt de PDF in `design/ref/` (alleen privé gebruik, niet in de publieke repo committen). Te controleren:
  - Pan stuurt naar ODD/EVEN-bussen en er zijn PGM-masters ODD en EVEN (zichtbaar op het apparaat). Hoe verhoudt dat zich tot de ASSIGN-knoppen en de stereomix?
  - Waar de AUX-PRE-aftakking precies zit (voor of na EQ).
  - Frequentiebereiken per EQ-band en de werking van de MONITOR/CUE-sectie.
  - Componentniveau-schema's zijn alleen nodig voor eventuele EQ-curves; het blokschema is de belangrijkste bron.

- Karakter per tapesoort (456-, 250- en 911-stijl): uitwerken tijdens het bouwen van het tape-model.
- Input monitoring en latencycompensatie bij opnemen: aanpak bepalen in fase 2.
- Apple Developer-account (99 dollar per jaar) voor signing/notarization bij delen, of accepteren dat gebruikers een waarschuwing krijgen.
- Is er een Windows-machine om te testen, of alleen via CI?
- Sneltoetsen en de exacte bediening van de concentrische EQ-knoppen (bijvoorbeeld slepen voor gain, slepen met modifier of scrollen voor frequentie).
- Maximale projectlengte en standaard tempo/maatsoort voor de metronoom.

---

## Fasering (voorstel)

1. Scaffold, CI-builds, projectbestanden
2. Audio-engine: apparaatkeuze, 8 sporen afspelen, transport
3. Opnemen per spoor met REC FUNCTION, punch in/out, metronoom
4. Mixer 8:8:2 met EQ, sends, assign, pan, faders
5. Bewerken: regio's, knippen/kopiëren/plakken/splitsen, undo
6. Effecten (plate reverb, tape echo) en tape-model
7. Export (bounce en stems), import, autosave, backups
8. Volledige vintage UI-skin volgens A2
9. Browserversie (WASM)
10. Delen: signing, releases

---

## Volgende stap voor Claude Code

**Instructie:**
> Bekijk eerst kort SoundCraft (https://github.com/storytold/soundcraft) op bruikbare architectuur en audiocode. Zet daarna de Cargo workspace op volgens de voorgestelde structuur, met PROJECT.md, ROADMAP.md (op basis van de fasering hierboven), CHANGELOG.md, versie 0.1.0, de MIT- en Apache-licentiebestanden en een GitHub Actions-workflow die voor macOS en Windows bouwt. Bouw vervolgens een eerste werkende versie: een egui-venster waarin je het audio-uitvoerapparaat kiest (cpal, met fallback naar het standaardapparaat), 8 WAV-bestanden op 8 sporen laadt en met PLAY, STOP en RTZ synchroon afspeelt via een eenvoudige stereomix. Maak de private repo `tonvanderkolk/daw-388` aan en push.

---

## Notities

- Dit project draait niet op Cloud86 en gebruikt geen PHP of MySQL.
- Audiothread: geen allocaties, locks of I/O; communicatie met de UI via lock-free queues.
- Houd de engine vrij van platformcode zodat de WASM-build later eenvoudig blijft.
- Standaardvoorkeuren van Ton: versienummer vanaf het begin, CHANGELOG bij elke wijziging, import/export en backups inbouwen, autosave met visuele bevestiging, na elke grote wijziging syncen via de project-sync skill.
- Documentatie in het Nederlands. Schrijfstijl: geen komma voor "en", "of" of "noch", geen em dash, geen emoticons.
- Geen wachtwoorden of tokens in dit bestand; die horen in CREDENTIALS.md (niet committen).
