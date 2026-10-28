# Ontwerp fase 7, stap 1: export (0.15.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek

## Doel

Een nummer uit DAW 388 halen: een stereo bounce en losse sporen, als WAV of AIFF. Fase 7 wordt opgedeeld: 0.15.0 export, 0.16.0 import van FLAC en MP3.

## Besluiten

| Vraag | Besluit |
|---|---|
| Bereik | Het hele nummer (begin tot einde van het langste spoor), of met een vinkje van het nulpunt tot het CUE-punt; altijd plus een staart voor galm en echo |
| Stems met mixer | Per spoor zoals in de mix: EQ, fader, pan en zijn eigen galm en echo, stereo |
| Hoe | Offline, sneller dan realtime, met een tweede engine op de achtergrond |

## Wat er geëxporteerd wordt

- **Stereo bounce**: de mix zoals je hem hoort (`Projectnaam_mix`). Vinkje "met tape-karakter" (standaard zoals het project staat).
- **Losse sporen**: per spoor aan of uit (standaard: sporen met audio), met een verwerking:
  - **met mixer en tape**: stereo; de mix met alleen dit spoor op SOLO (SOLO raakt geen opnames of bussen; de effecten krijgen alleen dit spoor). In de 388-stand is het spoor zijn L-R-kanaal en zijn MONITOR-rij, zoals SOLO werkt.
  - **alleen tape-karakter**: mono; het spoor door het tape-model, zonder mixer.
  - **helemaal droog**: mono; de opname zelf, alleen omgezet naar de exportsamplerate.
- **Alle sporen beginnen op 0:00**: bij een bereik vanaf het nulpunt begint elk bestand op dat punt (standaard aan). Uit: elk bestand begint op het begin van de band, ook als dat stilte is.
- Bestandsnamen `Projectnaam_01_Kick.wav` (de naam van de clip zonder extensie; onveilige tekens worden `_`), in de map `export/` van het project of een gekozen map. Een bestaand bestand krijgt een achtervoegsel (`-2`).

## Render

- Een tweede `Engine` op de exportsamplerate, zonder apparaat, op een achtergrondthread (`Platform::spawn`). Hij krijgt de sporen (via een goede resampler naar de exportsamplerate), de kanalen, MONITOR, de mixer, de effecten, de STEREO MASTER en het tape-model zoals in de app, plus het tempo voor de echo.
- Een kanaal op MIC of LINE zonder AUTO is stil (er is geen ingang); AUTO en RMX spelen de band.
- Na het einde van het bereik rendert de engine door tot de uitvoer 0,5 s onder -90 dBFS blijft, hooguit 10 s.
- Voortgang en annuleren via atomics; bij annuleren worden de al geschreven bestanden van deze export verwijderd.

## Formaat

- WAV of AIFF; 16 bit, 24 bit of 32 bit float; 44,1, 48 of 96 kHz. Standaard 24 bit, 48 kHz, WAV.
- 16 bit krijgt TPDF-dither; 24 bit en float niet. Waarden boven 0 dBFS worden in integerformaten begrensd en in float bewaard.
- Resampling met `rubato` (MIT) als de samplerate van een clip afwijkt; de bestaande Hermite-omzetting blijft voor het afluisteren.
- AIFF: 16 en 24 bit als AIFF, 32 bit float als AIFF-C (`fl32`).

## Bediening

- Knop **EXPORT** bij de transportsectie en "Exporteren..." in het menu Bestand openen het venster Export: stereo bounce aan/uit met tape-vinkje; losse sporen aan/uit, per spoor een vinkje, de verwerking; bereik; "alle sporen beginnen op 0:00"; formaat, bitdiepte, samplerate; map. Knop Exporteren, voortgangsbalk, Annuleren. Na afloop een melding met de map.
- De keuzes blijven bewaard zolang de app open is.

## Testen

- WAV en AIFF schrijven en teruglezen (16, 24, 32f; kanalen en samplerate kloppen); 16 bit heeft dither (een constante van een halve LSB geeft niet overal dezelfde waarde) en 24 bit niet.
- Resampling van 44,1 naar 48 kHz houdt een sinus op dezelfde frequentie en lengte.
- Een stereo bounce gelijk aan wat de engine live speelt (dezelfde commando's).
- Een droge stem bit voor bit gelijk aan de clip bij gelijke samplerate.
- De som van de stems met mixer gelijk aan de bounce (tape Clean, echo zonder feedback).
- Bereik nul tot CUE geeft de juiste lengte; de staart stopt na stilte.
- Annuleren laat geen bestanden achter.
