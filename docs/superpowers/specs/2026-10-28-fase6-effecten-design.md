# Ontwerp fase 6, stap 1: effecten (0.12.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek, wacht op beoordeling door Ton

## Doel

Twee ingebouwde effecten uit het tijdperk van de 388: een plate reverb op AUX en een tape echo op EFF, met EFFECT RTN 1 en 2 terug op de stereobus. Fase 6 wordt opgedeeld: 0.12.0 effecten, 0.13.0 tape-model, 0.14.0 dbx en Clean/388.

## Besluiten

| Vraag | Besluit |
|---|---|
| Fysiek of ingebouwd | Ingebouwd (software). Een fysiek effect kan al: AUX of EFF naar een interface-uitgang, terug via een kanaal op LINE |
| Bron van de plate | Altijd AUX, de echo altijd EFF, zoals op de 388. Wie AUX als koptelefoonmix gebruikt, draait RTN 1 dicht |
| Bediening | Een paar knoppen per effect: plate DECAY, PRE-DELAY, TONE; echo TIME, FEEDBACK, TONE |
| Echo-tijd | Vrij in milliseconden of SYNC op het metronoomtempo: 1/4, 1/8 of 1/8 gepunteerd |

## Signaalroute

- **Plate**: ingang is de AUX-som na de AUX MASTER, mono. Uitgang stereo, via EFFECT RTN 1 (GAIN, PAN als balans).
- **Echo**: ingang is de EFF-som na de EFF MASTER, mono. Uitgang mono, via EFFECT RTN 2 (GAIN, PAN constant power).
- Beide returns gaan naar de stereobus voor de STEREO MASTER, ook als de MONITOR-mix de stereobus vult (geen kanaal op L-R).
- **SOLO**: de AUX- en EFF-uitgangen naar de interface blijven ongemoeid. De effecten krijgen een eigen som waarin alleen de sporen meetellen die je hoort (SOLO toegepast). Zolang er niets gesoloed is, zijn beide sommen gelijk.
- **Meeluisteren uit**: een live bron gaat ook niet naar de effecten.
- Effecten staan nooit op een opname of een bus.

## Effecten (crate `fx`)

Alle buffers worden bij het aanmaken gereserveerd voor de samplerate van de engine (in `create`, niet in de audiothread). Geen allocaties, locks of I/O in `process`. Kleine waarden in de terugkoppeling worden naar 0 gezet tegen denormals.

### Plate

- Topologie naar Dattorro (1997): pre-delay, bandbreedtefilter, vier diffusie-allpasses, en een tank van twee helften (allpass, vertraging, demping, decay, allpass, vertraging) die elkaar voeden. Uitgang links en rechts uit vaste aftakkingen van beide helften. Vertragingen geschaald van 29 761 Hz naar de samplerate.
- **DECAY** 0,5 tot 6 s (standaard 2 s): de terugkoppelfactor volgt uit de gewenste nagalmtijd, begrensd op 0,97.
- **PRE-DELAY** 0 tot 100 ms (standaard 20 ms).
- **TONE** 0 tot 1 (standaard 0,5): demping in de tank, van donker naar helder.

### Echo

- Een vertraging van maximaal 2 s met lineaire interpolatie.
- **TIME** 40 tot 800 ms (standaard 300 ms). De werkelijke tijd glijdt langzaam naar de ingestelde (bandsnelheid die verandert), zodat draaien een toonhoogtebuiging geeft in plaats van tikken.
- **SYNC**: UIT, 1/4, 1/8, 1/8 gepunteerd. Dan volgt de tijd uit het metronoomtempo (een kwartnoot is `beat_seconds × unit / 4`), begrensd op 2 s.
- **FEEDBACK** 0 tot 0,9 (standaard 0,35).
- **TONE** 0 tot 1 (standaard 0,5): een laagdoorlaat in de lus (ongeveer 1,5 tot 9 kHz) plus een vaste hoogdoorlaat op 100 Hz; elke herhaling gaat ook door een zachte verzadiging. Een heel lichte wow (0,4 Hz, ongeveer 0,3 ms diep).
- Uitgang: alleen het vertraagde signaal (wet).

## Engine

- `MixerParams` krijgt `plate: PlateParams`, `echo: EchoParams` en `rtn: [RtnParams; 2]` (lineaire gain en pan).
- Per frame: de effectsommen (met SOLO) maal de masters gaan de effecten in; de returns worden met overgevloeide gains bij de stereobus opgeteld.
- `SetMeter` geeft de echo zijn tempo; de engine rekent de SYNC-tijd uit.

## Projectbestand

- `mixer.effects`: `plate` (`decay_s`, `predelay_ms`, `tone`), `echo` (`time_ms`, `sync` off/quarter/eighth/dotted_eighth, `feedback`, `tone`), `rtn` (twee keer `gain_db` en `pan`).
- Ontbreekt `effects` (project van 0.11.0 of ouder), dan staan de effectinstellingen op hun standaard en de RTN's dicht, zodat een koptelefoonmix op AUX niet ineens galm op de hoofduitgang geeft. Een nieuw project krijgt RTN op 0 dB.
- Formaatversie blijft 2.

## Bediening

In de mastersectie, boven AUX/EFF, in beide standen:

- **PLATE (AUX)**: DECAY, PRE en TONE; daaronder RTN 1 GAIN en PAN.
- **ECHO (EFF)**: TIME, FEEDBACK en TONE; schakelaars SYNC UIT, 1/4, 1/8, 1/8.; daaronder RTN 2 GAIN en PAN. Met SYNC aan is TIME gedimd en toont de tooltip de berekende tijd.
- Dubbelklik zet elke knop op zijn standaard; tooltips tonen de waarde.

## Testen

- **Plate**: een impuls geeft een stereo staart (links en rechts verschillen) die uitsterft; langere DECAY geeft meer energie na 1 s; op de hoogste stand en TONE helder blijft de uitgang begrensd en sterft hij uit; stilte in geeft na een tijd exact 0.
- **Echo**: een impuls komt terug op TIME (binnen een paar samples) en de tweede herhaling is ongeveer FEEDBACK keer de eerste (TONE en verzadiging meegerekend, ruime marge); SYNC 1/8 bij 120 BPM is 250 ms; FEEDBACK 0,9 blijft begrensd.
- **Engine**: AUX via de plate bereikt de stereobus; RTN dicht geeft stilte; SOLO op een ander spoor haalt een spoor uit de galm maar niet uit de AUX-uitgang; de echo op EFF komt na TIME terug.
- **Project**: zonder `effects` zijn de RTN's dicht; een nieuw project heeft RTN op 0 dB; opslaan en openen bewaart alles.
