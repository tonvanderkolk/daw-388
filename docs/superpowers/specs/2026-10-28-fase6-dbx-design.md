# Ontwerp fase 6, stap 3: dbx (0.14.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek

## Doel

De dbx-ruisonderdrukking van de 388, met het hoorbare ademen: ruis die in stiltes wegzakt en onder muziek meekomt. Per groep (sporen 1-4 en 5-8) aan of uit.

## Model

Omdat opnames schoon blijven, zit de hele keten in de weergave (crate `tape`), per spoor:

1. crosstalk
2. **dbx coderen** (als de groep dbx heeft): pre-emphasis (eerste orde), RMS-detector (10 ms), 2:1 compressie rond -20 dBFS; de detector heeft een bodem op -100 dBFS, zodat de versterking hooguit 40 dB is
3. saturatie en kleur (zoals 0.13.0)
4. ruis: zonder dbx -58 dBFS RMS bij 100% (snelheid en soort zoals 0.13.0), alleen op sporen met audio terwijl de band speelt
5. **dbx decoderen**: RMS-detector op wat van de band komt, 1:2 expansie, de-emphasis

Signaal: codering en decodering heffen elkaar vrijwel op. Ruis: wordt mee uitgezet, dus ver weg in stiltes en hoorbaar onder muziek (ademen). Kleur en saturatie in het gecodeerde deel geven een lichte mistracking, zoals echt.

Op Clean doet dbx niets.

## Projectbestand en bediening

- `tape.dbx`: twee booleans (1-4, 5-8), standaard aan; ook voor projecten van 0.13.0.
- Venster Tape: schakelaars "dbx 1-4" en "dbx 5-8".

## Testen

- dbx aan, saturatie en ruis uit: een sinus van 1 kHz op -20 dBFS komt binnen 0,5 dB overeen met dbx uit.
- Ruis in stilte met dbx minstens 20 dB lager dan zonder dbx.
- Ruis onder een signaal (op een buurloos spoor gemeten via het verschil met en zonder ruis) hoger dan in stilte: ademen.
- dbx 1-4 uit en 5-8 aan: spoor 1 heeft de ruis zonder dbx, spoor 5 met.
- Project: zonder `dbx` beide aan; opslaan en openen bewaart ze.
