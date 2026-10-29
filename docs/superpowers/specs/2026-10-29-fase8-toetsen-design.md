# Ontwerp fase 8, stap 2: toetsen zoals op de 388 (0.20.0)

Datum: 29-10-2026
Status: goedgekeurd in gesprek

## Doel

De knoppen in DAW 388 zijn nog egui-knopjes met tekst in een gekleurd vlak, en rode tekst op grijs leest slecht. Ze moeten lijken op de toetsen van een echte 388 (foto van Ton, 29-10-2026). Fase 8 loopt daarmee zo:

- **0.19.0**: meterbrug, bandrecorder, lettertype, meeschalen, MIXER en SPOREN (klaar)
- **0.20.0**: toetsen en de ingangsschakelaar (dit document)
- **0.21.0**: kanaalstrips en masters dichter bij de 388 (volgorde van de knoppen, fadercaps, nummer en naam onderaan)
- **0.22.0**: de vensters Tape, Export en Instellingen

## Besluiten

| Vraag | Besluit |
|---|---|
| Tekst | Boven de toets, klein en licht op het paneel, zoals op de 388; de toets zelf is leeg of heeft een symbool |
| Aan of uit | Een led bij de toets; de toets zelf verandert niet van kleur |
| MIC / LINE / RMX | Een schuifschakelaar met drie standen, zoals bovenaan een 388-kanaal |
| Knoppen die de 388 niet heeft | Dezelfde toetsvorm, zodat alles één geheel is |

## De toets

Een nieuwe module `crates/ui/src/skin/keys.rs` met één toets voor de hele app.

- **Vorm**: een vierkante of rechthoekige toets met hoeken van 2 punten, een donkere rand van 1 punt en een donkerder strook van 2 punten aan de onderkant, zodat hij verhoogd lijkt.
  - Ingedrukt (zolang de muisknop omlaag is): de toets zakt 1 punt en de strook verdwijnt.
  - Hover: een iets lichtere tint.
  - Uitgeschakeld: gedimd.
- **Kleuren** (`KeyColour`):
  - **Beige**: de meeste toetsen.
  - **Rood**: REC FUNCTION, REC, ASSIGN L-R, BOUNCE.
  - **Groen**: AUX PRE.
  - **Grijs**: de grote transporttoetsen, licht grijsbeige zoals op de foto.
- **Label**: boven de toets, gecentreerd, 9 punten, `TEXT_DIM`.
- **Led** (`Led`):
  - Uit, aan, of knipperend (twee keer per seconde, op `DawApp::now`).
  - Een stip van 3 punten links van het label, in de kleur die bij de functie hoort: groen voor REW, F FWD, PLAY en de standen, rood voor REC en REC FUNCTION, oranje voor MUTE en voor de metronoom.
  - Zonder led wordt er geen ruimte voor de stip gereserveerd.
- **Symbolen** (`Symbol`), getekend met de painter in donkergrijs:
  - REW: twee driehoekjes naar links, als omtrek.
  - F FWD: twee driehoekjes naar rechts.
  - STOP: een vierkantje.
  - PLAY: een driehoek.
  - REC: een gevulde rode stip.
- **Maten**: groot (transport), 44 × 34; klein (functietoetsen), 26 × 20; smal (ASSIGN en de strips), 22 × 14. De breedte groeit mee als een label breder is.
- **Aanroep**: `key(ui, &Key { label, colour, led, symbol, size })` geeft een `egui::Response` terug, zodat tooltips en `clicked()` werken zoals nu.

## De schuifschakelaar

`slide_switch(ui, labels, selected) -> Option<usize>` in dezelfde module.

- Een zwarte gleuf met een beige schuifje, drie standen.
- De labels (MIC, LINE, RMX) staan erboven op het paneel; de gekozen stand is helder, de andere gedimd.
- Klikken op een stand of het schuifje erheen slepen kiest hem.
- In de eenvoudige stand heeft hij twee standen (MIC, LINE), zoals nu de RMX-toets daar ontbreekt.

## Waar welke toets komt

**Transport** (MIXER en de bovenbalk van SPOREN):
- Een rij kleine toetsen met het label erboven: RESET, CUE, STC en REPEAT (led brandt als REPEAT aan staat).
- Daaronder vijf grote grijze toetsen met symbolen en leds: REW, F FWD, STOP, PLAY en REC.
  - De led van REC knippert bij record-ready en brandt tijdens het opnemen.
  - STOP heeft geen led.
- INSERT en AUTO PUNCH als kleine toetsen met een led.
- CLICK met een oranje led die op de tel meeknippert. TAP zonder led.
- RTZ als kleine toets.
- PITCH VARI/FIX als twee kleine toetsen met een led bij de gekozen stand.
- BOUNCE als rode toets en EXPORT als beige toets, beide groter, met het label erboven.
- De tape-regelknop ("TAPE: ...") wordt een beige toets met het label TAPE erboven; de tekst over band en snelheid staat al boven de spoelen.

**Kanaalstrip**:
- INPUT als schuifschakelaar; de keuze voor de ingang (in 1, in 2...) blijft een keuzelijst.
- LAAD en WIS als smalle beige toetsen.
- REC (REC FUNCTION) als rode toets met een led die knippert als het spoor klaarstaat en brandt tijdens het opnemen.
- MUTE (oranje led) en SOLO (groene led) als smalle beige toetsen.
- AUX PRE als smalle groene toets met een led bij PRE.
- In de 388-stand ASSIGN 1-2, 3-4, 5-6 en 7-8 als smalle beige toetsen en L-R als rode, elk met een led. DIRECT en AUTO als smalle beige toetsen met een led.

**Master en MONITOR**:
- STEREO, AUX en EFF (luisteren) en METER L-R en AUX-EFF als kleine toetsen met een led bij de gekozen stand.
- TRACK METERS PRE en POST, MONO, MON PGM/CUE en de M per MONITOR-rij in dezelfde stijl.
- De echo-synchronisatie (VRIJ, 1/4, 1/8, 1/8.) als kleine toetsen met een led.

**SPOREN**:
- De werkbalk (SPLITS, KNIP, KOPIEER, PLAK, WIS, RASTER, −, PAST, +) in dezelfde stijl, met het label op het paneel erboven.
- RASTER en PAST met een led.

**Wat blijft**:
- Keuzelijsten en invoervelden (in 1, BPM, maatsoort, uitvoerapparaat) blijven egui-widgets. Ze krijgen via de visuals in `apply_style` dezelfde kleuren: een donker veld met een beige rand en lichte tekst.
- De kopregel (Bestand, Bewerken, Eenvoudig/388) blijft in deze stap zoals hij is.

## Code

- `skin/keys.rs`: `Key`, `KeyColour`, `Led`, `Symbol`, `key()`, `slide_switch()`, en de pure hulpfuncties `led_lit(led, now)` en `switch_index(x, rect, count)`.
- `toggle`, `small_key` en `transport_key` in `lib.rs` verdwijnen. Elke aanroep gaat naar `key()`; daarmee valt ook de rode tekst op grijs weg.
- De losse `egui::Button`-aanroepen voor REC, BOUNCE, EXPORT en Laad/Wis gaan ook naar `key()`.
- De kleuren van de toetsen komen in `skin::palette`.

## Testen

- `led_lit`: uit, aan, en knipperen op twee keer per seconde (aan in de eerste helft van elke halve seconde).
- `switch_index`: een klik links, midden en rechts kiest stand 0, 1 en 2; buiten de gleuf valt hij binnen het bereik.
- De bestaande tests van het transport en de mixer blijven slagen. Ze gebruiken de functies achter de toetsen, niet de toetsen zelf.
- Het uiterlijk test Ton in de app.

## Buiten deze stap

- De indeling van de strips, de volgorde van de knoppen en de fadercaps: 0.21.0.
- De vensters Tape, Export en Instellingen en de kopregel: 0.22.0.
