# Ontwerp fase 6, stap 2: tape-model (0.13.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek

## Doel

Het karakter van de 388-band: bandsnelheid, tapesoort, saturatie, wow en flutter, ruis en crosstalk, met een schakelaar Clean / 388. dbx volgt in 0.14.0.

## Besluiten

| Vraag | Besluit |
|---|---|
| Waar zit het tape-karakter | Alleen in de weergave van de band. Opnames van een ingang blijven schoon en het karakter is altijd aan te passen of uit te zetten |
| Bediening | Een venster Tape: Clean / 388, snelheid 3¾ / 7½ / 15 IPS, soort 456 / 250 / 911, en SATURATIE, WOW & FLUTTER, RUIS en CROSSTALK van 0 tot 200% (100% is de machine). Op het paneel een knop TAPE met de huidige snelheid en soort |
| Clean / 388 | Komt al in 0.13.0, als aan/uit van het tape-model |
| Startstand | Nieuw project: 388, 7½ IPS, 456, alles 100%. Een project van 0.12.0 of ouder: Clean |

## Signaalroute

- Het model bewerkt de weergave van alle acht sporen samen (de band), voor RMX, AUTO en de MONITOR-sectie.
- Wat live van de ingang komt, heeft geen tape-karakter. Een pingpong of bounce van band naar band neemt de weergave op en dus ook het karakter, zoals op een echte machine (generatieverlies). Op Clean niet.
- Op Clean gaat de band bit voor bit door, zonder wow-verschuiving.

## Model (crate `tape`)

Per spoor, in deze volgorde:

1. **Crosstalk**: de twee buursporen lekken mee op -75 dB maal CROSSTALK (uit de waarden voor deze stap, zodat het lek niet rondzingt).
2. **Saturatie**: een eerste-orde pre-emphasis (het hoog verzadigt eerder), een zachte, licht asymmetrische tanh-kromme met een aansturing per tapesoort maal SATURATIE, en de exacte omgekeerde de-emphasis. Bij SATURATIE 0 wordt deze stap overgeslagen. Zachte signalen blijven vrijwel lineair.
3. **Kleur**: head bump (piekfilter +1,5 dB, Q 1) op 40, 60 of 100 Hz voor 3¾, 7½ of 15 IPS; afrol in het hoog (laagdoorlaat, Q 0,707) op 10, 15 of 20 kHz, begrensd op 0,45 maal de samplerate; tapesoort: 250 een piekfilter -1 dB op 8 kHz (warm), 911 +1 dB (helder), 456 vlak.
4. **Ruis**: alleen op sporen met audio en alleen als de band speelt. Gefilterde witte ruis, 100% is -80 dBFS RMS; 3¾ IPS +3 dB, 15 IPS -3 dB, tapesoort 250 +2 dB.

**Wow en flutter** gelden voor de hele band: de afspeelpositie krijgt een kleine verschuiving uit een langzame (0,5 Hz) en een snelle (6 Hz) modulatie. 100% is ongeveer 0,05% WRMS bij 7½ IPS; bij 3¾ IPS twee keer zoveel, bij 15 IPS 0,6 keer.

Aansturing per tapesoort: 456 1,0; 250 1,6 (verzadigt eerder); 911 1,2.

Alles zonder allocaties in de audiothread; ruis uit een eigen xorshift-generator per spoor.

## Engine

- `Command::SetTape(TapeParams)`; standaard Clean, zodat bestaande tests onveranderd blijven.
- Per frame: als de band speelt en het model aan staat, leest de engine elk spoor op `position + wow`, en geeft de acht waarden aan `TapeMachine::process`.

## Projectbestand

- Bovenaan `tape`: `mode` (`clean` of `388`; ontbreekt: `clean`), `speed` (`3.75`, `7.5`, `15`), `kind` (`456`, `250`, `911`), `saturation`, `wow_flutter`, `noise`, `crosstalk` (0 tot 2, standaard 1).
- Formaatversie blijft 2.

## Bediening

- Knop **TAPE** bij de transportsectie met de huidige stand (bijvoorbeeld "7½ IPS 456" of "CLEAN"); klik opent het venster. Ook in het menu Bestand: "Tape-instellingen...".
- Venster **Tape**: Clean / 388; snelheid; soort; vier schuiven in procenten met dubbelklik naar 100%. Met Clean zijn snelheid, soort en schuiven gedimd.

## Testen

- Clean geeft de band bit voor bit door, ook met wow op 200%.
- 15 IPS houdt meer hoog over dan 3¾ (een sinus van 12 kHz).
- Saturatie: een zacht signaal (-30 dBFS) blijft binnen 0,1 dB; een hard signaal wordt afgeplat en blijft begrensd; 250 verzadigt meer dan 456.
- Wow en flutter: de verschuiving blijft binnen een paar samples en is 0 op 0%.
- Ruis: alleen als de band speelt en het spoor audio heeft; op 0% stil.
- Crosstalk: een signaal op spoor 2 verschijnt op spoor 1 en 3 rond -75 dB, niet op spoor 4.
- Engine: met 388 aan bevat een opname van de ingang geen ruis of wow; de weergave wel.
- Project: zonder `tape` Clean; een nieuw project 388; opslaan en openen bewaart alles.
