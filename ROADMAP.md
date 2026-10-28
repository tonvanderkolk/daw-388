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
- [ ] Opnemen met de pitch-knop op VARI

## Fase 4: Mixer 8:8:2
- [x] Ingangskeuze MIC / LINE / TAPE, trim (0.8.0; voorversterker-karakter in 0.9.0)
- [x] 3-band sweepable EQ (concentrische knoppen) (0.9.0)
- [x] AUX-send met PRE/POST, EFF-send post-fader (0.11.0)
- [x] ASSIGN 1-2, 3-4, 5-6, 7-8 en L-R, busrouting (0.8.0)
- [x] MONITOR-sectie met de omschakellogica van de 388, MON PGM/CUE, MONO (0.8.0)
- [x] Eenvoudige stand naast de 388-routering, met AUTO per kanaal (0.10.0)
- [x] SOLO per spoor, alleen voor wat je hoort (0.10.1)
- [x] Master-sectie: MONITOR/PHONES, AUX- en EFF-masters naar interface-uitgangen, STEREO/AUX/EFF, METER, STEREO MASTER (0.11.0; EFFECT RTN in fase 6)
- [ ] Signaalroute controleren met het blokschema uit de handleiding

## Fase 5: Bewerken
- [ ] Niet-destructieve regio's: verschuiven, knippen, kopiëren, plakken, splitsen
- [ ] Undo (maximaal 10 stappen)
- [ ] Ping-pong bounce naar een vrij spoor (kan via de bussen sinds 0.8.0)

## Fase 6: Effecten en tape-model
- [x] Plate reverb (aux) en tape echo (eff), met EFFECT RTN (0.12.0)
- [x] Tape-model 388 (0.13.0): 3¾, 7½ en 15 IPS, tapesoorten 456-, 250- en 911-stijl
- [x] Saturatie, wow en flutter, ruis, crosstalk (0.13.0; varispeed is de pitch-knop sinds fase 2)
- [x] Schakelaar Clean / 388 (0.13.0)
- [x] dbx per groep, met het ademen van de ruis (0.14.0)

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
