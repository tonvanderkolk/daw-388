# Ontwerp fase 8, stap 3: strips en master zoals de 388 (0.21.0)

Datum: 29-10-2026
Status: goedgekeurd in gesprek

## Doel

Sinds 0.20.0 zijn de toetsen goed, maar de console oogt onrustig. Elke strip heeft dezelfde onderdelen op net andere hoogtes, knoppen staan naast elkaar waar ze op de 388 onder elkaar staan, en de master-sectie is een stapel losse blokjes. De echte 388 (foto van Ton, 29-10-2026) lost dat zo op:

- vaste vlakken, gescheiden door dunne lichte lijnen
- alles onder elkaar
- het nummer onder de fader
- twee nette kolommen in de master-sectie

0.21.0 neemt die indeling over.

## Besluiten

| Vraag | Besluit |
|---|---|
| Indeling van een strip | Vaste vlakken onder elkaar, op exact dezelfde hoogte in alle strips, gescheiden door dunne lijnen |
| REC FUNCTION | Blijft in de strip, in het vlak boven de fader samen met MUTE en SOLO |
| Wat de 388 niet heeft | Eigen vlakken: bovenaan de strip SPOOR (LAAD, WIS, ingang), boven de fader REC/MUTE/SOLO, en een paneel EFFECTEN onder het transport |

## De kanaalstrip

Van boven naar beneden, elk vlak met een vaste hoogte. Een vlak dat in een stand niet bestaat (ASSIGN in de eenvoudige stand), valt in alle strips tegelijk weg.

| Vlak | Inhoud |
|---|---|
| SPOOR | LAAD en WIS naast elkaar, daaronder de keuzelijst voor de ingang (in 1, in 2...). Bij RMX zonder AUTO blijft de ruimte van de keuzelijst leeg, zodat er niets verschuift |
| INPUT | De schuifschakelaar MIC / LINE / RMX met het label INPUT, daaronder het OL-lampje en de TRIM-knop (rood), zoals op de 388 |
| EQ | HIGH, MID en LOW onder elkaar als concentrische knoppen (binnen gain, buiten frequentie), met het label links van elke knop |
| AUX | De PRE-toets (groen) en de AUX-knop (groen) naast elkaar, daaronder de EFF-knop (blauw) |
| ASSIGN | Alleen in de 388-stand. De toetsen 1-2, 3-4, 5-6 en 7-8 onder elkaar met L-R (rood) eronder. Rechts ervan DIRECT en AUTO onder elkaar |
| PAN | De PAN-knop (crème) met ODD en EVEN eronder in de 388-stand, L en R in de eenvoudige stand |
| REC / MUTE / SOLO | De drie toetsen naast elkaar |
| FADER | De fader, de waarde in dB eronder, dan het spoornummer groot en de naam van het spoor klein |

- **Lijnen**: tussen de vlakken een horizontale lijn van 1 punt in `TEXT_DIM` op 35%, over de volle breedte van de strip. De verticale scheidingslijnen tussen de strips blijven.
- **Labels**: elk vlak heeft links bovenin een label in 9 punten (INPUT, EQ, AUX, ASSIGN, PAN), zoals op de 388.
- **Knoppen**: één maat per soort. TRIM, EQ, AUX, EFF en PAN zijn 30 punten; de EQ-knoppen worden door hun buitenring iets groter.
- **Hoogte**: de vlakhoogtes staan als constanten in de code. Elk vlak gebruikt precies zijn hoogte (`allocate_ui` met een vaste maat), zodat een vlak met minder inhoud niets laat verschuiven. Daarmee is het uitlijnen van de faders via de vorige frame (`fader_row`) niet meer nodig.
- **Breedte**: de strip blijft 112 punten.

## De master-sectie

Twee kolommen, met dezelfde vlakken en lijnen als de strips. De faders onderaan staan op de hoogte van de kanaalfaders.

**Kolom EFFECT RTN en MONITOR**:
- EFFECT RTN: twee rijen (1: plate, 2: echo) met GAIN (blauw) en PAN.
- MONITOR, alleen in de 388-stand: acht rijen met het spoornummer, GAIN, PAN, de M-toets en het TRK/PGM-balkje. Daaronder MON PGM/CUE.
- Onderaan de PGM MASTERS-faders ODD en EVEN (oranje), alleen in de 388-stand.
- In de eenvoudige stand blijft alleen EFFECT RTN over.

**Kolom MASTER**:
- MASTER AUX en EFF (de send-masters).
- METER L-R / AUX-EFF, TRACK METERS PRE/POST en MONO, als toetsen onder elkaar.
- Onderaan de STEREO MASTER (rood) met de waarde in dB.

## Het paneel EFFECTEN

Onder het transport, rechts op de console, waar de 388 leeg is. Een vlak met dezelfde lijnen, met:
- PLATE: DECAY, PRE en TONE
- ECHO: TIME, FEEDB en TONE, met de synchronisatietoetsen VRIJ, 1/4, 1/8 en 1/8.
- LUISTEREN: STEREO, AUX en EFF, en de PHONES-knop

## Code

- **Vlakken**: in `lib.rs` een hulpfunctie `section(ui, label, height, add)`. Die tekent het label en de lijn eronder en geeft de inhoud precies `height` punten; de hoogtes staan als constanten (`SECTION_*`).
- **Strip**: `channel_strip` wordt opgedeeld per vlak. Bijvoorbeeld `strip_input`, `strip_eq`, `strip_aux`, `strip_assign`, `strip_pan`, `strip_keys`, `strip_fader`. Elk tekent zijn vlak met `section`.
- **Master-sectie**:
  - `pgm_masters`, `monitor_section` en `master_section` worden `return_monitor_column` en `master_column`.
  - De bediening van plate en echo verhuist uit `effects_controls` naar een nieuwe `effects_panel` naast het transport. De RTN-knoppen gaan naar de kolom EFFECT RTN.
- **Uitlijnen**: `fader_row`, `fader_row_next` en `to_fader_row` verdwijnen, omdat de vaste vlakhoogtes de faders vanzelf op één lijn zetten.
- Het meten voor de zoom (`needed`) blijft zoals het is.

## Testen

- **Hoogtes**: een unit-test rekent de som van de vlakhoogtes per stand uit. In de 388-stand en de eenvoudige stand begint de fader van de strips en die van de master-kolom op dezelfde hoogte.
- **Bestaande tests**: die van mixer, transport en opnemen blijven slagen; ze gebruiken de functies achter de bediening.
- **Uiterlijk**: dat test Ton in de app.

## Buiten deze stap

- De vensters Tape, Export en Instellingen en de kopregel: 0.22.0.
- De fadercaps en de schaal van de faders blijven zoals ze zijn.
