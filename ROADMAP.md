# ROADMAP - DAW 388

## Fase 1: Scaffold, CI-builds, projectbestanden (0.1.x)
- [x] Cargo workspace met engine, tape, fx, project, ui en app-desktop
- [x] PROJECT.md, ROADMAP.md, CHANGELOG.md, licenties (MIT OR Apache-2.0)
- [x] GitHub Actions-build voor macOS en Windows
- [x] Eerste werkende versie: uitvoerapparaat kiezen, 8 WAV-bestanden laden, synchroon afspelen met PLAY, STOP en RTZ
- [x] `project.json` met een `audio/`-submap opslaan en openen (0.2.0)

## Fase 2: Audio-engine
- [x] Laden en samplerate-conversie op een achtergrondthread (0.4.0)
- [ ] Stille klok als er geen apparaat is
- [ ] Herstel na het wegvallen van het apparaat
- [ ] Keuze van samplerate en buffergrootte
- [x] Transport compleet: RESET, CUE, STC, REPEAT, REW, F FWD, teller in min:sec, pitch/varispeed (0.4.0)
- [x] REPEAT-modus 388 (spoelt terug) of cycle (springt direct terug), per project (0.4.0)

## Fase 3: Opnemen
- [ ] Ingangsapparaat kiezen, REC FUNCTION per spoor (arm-knoppen met lampje)
- [ ] Opnemen naar WAV in de projectmap
- [ ] Punch in/out, metronoom
- [ ] Input monitoring en latencycompensatie

## Fase 4: Mixer 8:8:2
- [ ] Ingangskeuze MIC / LINE / TAPE, trim
- [ ] 3-band sweepable EQ (concentrische knoppen)
- [ ] AUX-send met PRE/POST, EFF-send post-fader
- [ ] ASSIGN 1-2, 3-4, 5-6, 7-8 en L-R, busrouting
- [ ] Master-sectie: EFFECT RTN, MONITOR, AUX- en EFF-masters, STEREO MASTER
- [ ] Signaalroute controleren met het blokschema uit de handleiding

## Fase 5: Bewerken
- [ ] Niet-destructieve regio's: verschuiven, knippen, kopiëren, plakken, splitsen
- [ ] Undo (maximaal 10 stappen)
- [ ] Ping-pong bounce naar een vrij spoor

## Fase 6: Effecten en tape-model
- [ ] Plate reverb (aux) en tape echo (eff)
- [ ] Tape-model 388: 3¾, 7½ en 15 IPS, tapesoorten 456-, 250- en 911-stijl
- [ ] Saturatie, wow en flutter, ruis, crosstalk, varispeed
- [ ] dbx per groep, schakelaar Clean / 388

## Fase 7: Export, import, autosave
- [ ] Stereo bounce en stems, verwerkingsopties, WAV/AIFF, 16/24/32-bit float, 44,1/48/96 kHz
- [ ] Import van andere formaten (symphonia): AIFF is er sinds 0.2.1, FLAC en MP3 volgen
- [x] Autosave met visuele bevestiging, backups (0.2.0)

## Fase 8: Vintage UI-skin volgens A2
- [ ] Meterbrug met VU-meters, bandrecorder met spoelen, houten omlijsting
- [ ] Bewerkingspaneel met 8 sporen en tijdlijn
- [ ] Vensters voor tape-instellingen en export

## Fase 9: Browserversie (WASM)
- [ ] WebAudio-koppeling, lite-versie met cross-origin isolation

## Fase 10: Delen
- [ ] Signing en notarization (macOS), releases via GitHub
