# ROADMAP - DAW 388

## Fase 1: Scaffold, CI-builds, projectbestanden (0.1.x)
- [x] Cargo workspace met engine, tape, fx, project, ui en app-desktop
- [x] PROJECT.md, ROADMAP.md, CHANGELOG.md, licenties (MIT OR Apache-2.0)
- [x] GitHub Actions-build voor macOS en Windows
- [x] Eerste werkende versie: uitvoerapparaat kiezen, 8 WAV-bestanden laden, synchroon afspelen met PLAY, STOP en RTZ
- [x] `project.json` met een `audio/`-submap opslaan en openen (0.2.0)

## Fase 2: Audio-engine
- [x] Laden en samplerate-conversie op een achtergrondthread (0.4.0)
- [x] Stille klok als er geen apparaat is (0.5.0)
- [x] Herstel na het wegvallen van het apparaat (0.5.0)
- [x] Keuze van samplerate en buffergrootte (0.5.0)
- [x] Transport compleet: RESET, CUE, STC, REPEAT, REW, F FWD, teller in min:sec, pitch/varispeed (0.4.0)
- [x] REPEAT-modus 388 (spoelt terug) of cycle (springt direct terug), per project (0.4.0)

## Fase 3: Opnemen
- [x] Ingangsapparaat kiezen, ingang per spoor, REC FUNCTION per spoor (lampje knippert klaar, brandt tijdens opname) (0.6.0)
- [x] REC, record-ready, punch-in en punch-out zoals op de 388, INSERT, uitpunchen bij het CUE-punt (0.6.0)
- [x] AUTO PUNCH van nulpunt tot CUE-punt, ook herhaald met REPEAT (0.6.0)
- [x] Opnemen naar 24-bit WAV in de projectmap, destructief zoals band maar terug te draaien met Cmd+Z (0.6.0)
- [x] Meeluisteren via DAW 388 of via de interface, latencycompensatie met handmatige fijnafstemming (0.6.0)
- [x] Metronoom: klik, tempo, tap tempo, maatsoort, aftellen, maat en tel in de teller (0.7.0)
- [x] Opnemen met de pitch-knop op VARI (0.23.0)

## Fase 4: Mixer 8:8:2
- [x] Ingangskeuze MIC / LINE / TAPE, trim (0.8.0; voorversterker-karakter in 0.9.0)
- [x] 3-band sweepable EQ (concentrische knoppen) (0.9.0)
- [x] AUX-send met PRE/POST, EFF-send post-fader (0.11.0)
- [x] ASSIGN 1-2, 3-4, 5-6, 7-8 en L-R, busrouting (0.8.0)
- [x] MONITOR-sectie met de omschakellogica van de 388, MON PGM/CUE, MONO (0.8.0)
- [x] Eenvoudige stand naast de 388-routering, met AUTO per kanaal (0.10.0)
- [x] SOLO per spoor, alleen voor wat je hoort (0.10.1)
- [x] Master-sectie: MONITOR/PHONES, AUX- en EFF-masters naar interface-uitgangen, STEREO/AUX/EFF, METER, STEREO MASTER (0.11.0; EFFECT RTN in fase 6)
- [x] Signaalroute controleren met het blokschema uit de handleiding (0.23.2: alles klopt, op de TRK-meters na; die volgen nu de MONITOR)

## Fase 5: Bewerken
- [x] Niet-destructieve regio's: verschuiven, knippen, kopiëren, plakken, splitsen (0.17.0 en 0.18.0)
- [x] Undo (maximaal 10 stappen) (0.17.0, met redo)
- [x] Ping-pong bounce naar een vrij spoor (0.8.0: in de 388-stand via ASSIGN en de bussen, zie PROJECT.md)

## Fase 6: Effecten en tape-model
- [x] Plate reverb (aux) en tape echo (eff), met EFFECT RTN (0.12.0)
- [x] Tape-model 388 (0.13.0): 3¾, 7½ en 15 IPS, tapesoorten 456-, 250- en 911-stijl
- [x] Saturatie, wow en flutter, ruis, crosstalk (0.13.0; varispeed is de pitch-knop sinds fase 2)
- [x] Schakelaar Clean / 388 (0.13.0)
- [x] dbx per groep, met het ademen van de ruis (0.14.0)

## Fase 7: Export, import, autosave
- [x] Stereo bounce en stems, verwerkingsopties, WAV/AIFF, 16/24/32-bit float, 44,1/48/96 kHz (0.15.0)
- [x] Import van andere formaten (symphonia): AIFF sinds 0.2.1, FLAC en MP3 sinds 0.16.0
- [x] Autosave met visuele bevestiging, backups (0.2.0)

## Fase 8: Vintage UI-skin volgens A2
- [x] Meterbrug met VU-meters, bandrecorder met spoelen, houten omlijsting (0.19.0)
- [x] Bewerkingspaneel met 8 sporen en tijdlijn (0.18.0; de vintage vormgeving volgt)
- [x] Twee weergaven: MIXER en SPOREN, met volume en pan per spoor (0.19.0)
- [x] Toetsen zoals op de 388 (0.20.0)
- [x] Kanaalstrips en masters in vintage vorm (0.21.0)
- [x] Kopregel, vensters voor tape-instellingen, export en settings, LED-displays (0.22.0)

## Fase 9: Browserversie (WASM)
- [ ] WebAudio-koppeling, lite-versie met cross-origin isolation

## Fase 10: Delen
- [x] macOS-app-bundel met icoon, en het projectpakket `.daw388` aangemeld in de Info.plist (`LSTypeIsPackage`), zodat de Finder een project als één document toont (0.25.0)
- [x] Releases via GitHub: universal DMG voor macOS, ZIP voor Windows, met icoon (0.25.0)
- [x] Help-menu met About en een korte gids in de app (0.26.0)
- [x] Compacte console voor lage schermen zoals laptops van 1920 bij 1080 (0.28.0)
- [x] Gebruikershandleiding als PDF bij de app en de release, te openen via Help (0.29.0)
- [x] Donatielink naar Buy Me a Coffee in de app, de README en op GitHub (0.30.0)
- [ ] Signing en notarization (macOS): bewust niet, zonder Apple Developer-account; gebruikers openen de app de eerste keer via Privacy en beveiliging. Eventueel later een jaar lidmaatschap voor een stabiele versie: ondertekende versies blijven daarna werken
- [ ] Windows: ASIO-ondersteuning voor lage vertraging met audio-interfaces (cpal-feature `asio`, vraagt de ASIO SDK bij het bouwen)
- [ ] Windows op ARM: een eigen `aarch64-pc-windows-msvc`-build (nu via x64-emulatie, niet getest)
- [x] Repository openbaar gemaakt (31-10-2026)
- [x] Controleren op updates (0.31.0): bij het starten op de achtergrond (hooguit één keer per dag, uit te zetten in Settings) de laatste release opvragen bij de GitHub-API (`releases/latest`) en vergelijken met de eigen versie. Is er een nieuwere, dan een melding in de statusregel en een item in Help dat de releasepagina opent; daarnaast Help, Check for Updates.... Geen automatische installatie (de app is niet ondertekend). Vraagt een kleine HTTP-bibliotheek (`ureq`); ook in de handleiding vermelden
- [x] Een project openen door erop te dubbelklikken in de Finder (0.31.1): de app-delegate van winit krijgt zelf `application:openURLs:`
- [x] Opslaan-vraag bij afmelden, herstarten en Stop in het Dock-menu op macOS (0.32.0): de app-delegate van winit krijgt `applicationShouldTerminate:`
- [x] Undo van 10 naar 50 stappen (0.32.0)
- [x] Bestanden uit de Finder op het spoor en de tijd onder de muis, met een kader tijdens het slepen (0.32.0, macOS)
- [x] Pre-roll bij AUTO PUNCH: 1, 2 of 4 seconden voor het nulpunt, ook bij elke doorgang met REPEAT (0.32.0)
- [ ] Windows: de opslaan-vraag bij afmelden of afsluiten van Windows. winit beantwoordt `WM_QUERYENDSESSION` niet, dus de app stopt dan zonder vraag (autosave beperkt het verlies tot 30 seconden). Aanpak: het venster subclassen, bij niet-opgeslagen wijzigingen `FALSE` teruggeven met `ShutdownBlockReasonCreate` en de vraag tonen. Alleen als het nodig blijkt; vraagt een test op een Windows-pc

## Overwegingen voor later
- [ ] Afstandsbediening op een telefoon of tablet: de app draait (alleen als je het aanzet) een kleine webserver op het lokale netwerk en toont een QR-code met het adres en een geheime sleutel. De pagina in Safari of Chrome heeft het transport (REW, F FWD, STOP, PLAY, REC), de teller, REC FUNCTION per spoor en eventueel undo van de laatste take; geen faders, pan of mixer. Let op: de firewall van macOS en Windows vraagt om toestemming (bij een niet-ondertekende app mogelijk na elke update). Eerst de details uitwerken
- [ ] 16 sporen als keuze per project (8 blijft de standaard, zodat de 388 de 388 blijft). Engine en tape-model rekenen met `TRACKS` en schalen grotendeels mee; het werk zit in de console (te breed voor 16 strips: een bankschakelaar 1-8 en 9-16, ook voor de meterbrug en de MONITOR-sectie), de 388-routering (8 bussen: bus N naar spoor N en N+8, zoals een 8-bus console aan een 16-sporen recorder), de dbx-groepen (vier in plaats van twee), crosstalk, "Load 8 audio files", de TRACKS-weergave, de compacte console, het projectformaat (oude projecten openen als 8 sporen), tests en de handleiding. Ruwweg zoveel werk als de compacte console en de regio's samen
