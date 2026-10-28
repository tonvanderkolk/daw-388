# Ontwerp fase 4: mixer 8:8:2

Datum: 28-10-2026
Status: goedgekeurd in gesprek, nagelopen, wacht op beoordeling door Ton

## Doel

De mixer van DAW 388 volgt de signaalroute van de Tascam 388 (handleiding, Features and Controls en Monitor Submix), met één afwijking: een DIRECT-schakelaar per spoor als snelle opnameroute naast de busroute. Fase 4 levert kanaalstroken, PGM-bussen, de MONITOR-sectie, AUX en EFF en de mastersectie. Effecten (plate reverb en tape echo) volgen in fase 6; EFFECT RTN is tot dan stil.

## Besluiten

| Vraag | Besluit |
|---|---|
| Hoe trouw aan de 388 | Echte busroute, plus DIRECT per spoor (hybride) |
| MIC en LINE | Eén interface-ingang per kanaal. MIC gaat door een nagebootste 388-voorversterker, LINE is schoon. RMX is de band |
| DIRECT neemt op | Kanaal N na TRIM en EQ, voor de fader |
| AUX en EFF tot fase 6 | Naar een vrije interface-uitgang naar keuze; EFFECT RTN wacht op fase 6 |
| EQ-knop | Slepen of scrollen in het midden is gain, op de buitenring frequentie; dubbelklik zet gain op 0 dB |
| Startstand van een project | Opnamestand: L-R uit, je hoort de sporen via de MONITOR-sectie. L-R is voor de remix |
| Code-opzet | Modules `mixer.rs` en `monitor.rs` in de engine-crate, één commando `SetChannel` per kanaal |

## Begrippen

- **Live bron**: een kanaal op MIC of LINE met een gekozen interface-ingang.
- **Spoor N luistert naar zijn ingang** (`hears_input(N)`): spoor N staat op REC FUNCTION en neemt op, of staat op REC FUNCTION terwijl INSERT uit staat. Dit is de vroegere `hears_input` zonder de voorwaarde over het transport en zonder de voorwaarde dat er een ingang is.
- **Spoor N neemt op** (`recording[N]`): de bestaande toestand per spoor (na punch-in, voor punch-out), niet de REC-stand van de machine.
- **Meeluisteren** is de bestaande app-voorkeur `software_monitoring` (meeluisteren via DAW 388 of via de interface).

## Signaalroute

### Kanaal 1 tot 8

1. **Bron**
   - **MIC**: de interface-ingang van het kanaal door de voorversterker. TRIM 0 tot +40 dB. Boven ongeveer -6 dBFS begint zachte, licht asymmetrische verzadiging (tanh-achtig, wat even harmonischen), begrensd onder 0 dBFS plus marge. Geen ruis.
   - **LINE**: dezelfde ingang, schoon. TRIM -10 tot +20 dB.
   - **RMX**: weergave van spoor N, TRIM -10 tot +20 dB, schoon. Stil zolang spoor N naar zijn ingang luistert (zie Begrippen); zo ontstaat er geen terugkoppeling van bus via spoor naar kanaal. Met INSERT speelt RMX de band dus tot de punch-in.
   - MIC of LINE zonder gekozen ingang, of zonder ingangsapparaat, geeft stilte.
2. **TRIM**
3. Aftakking **AUX PRE** (voor de EQ, zoals de handleiding zegt: voor EQ, ACCESS en fader)
4. **EQ**: drie piekfilters (RBJ-biquad, Q ongeveer 0,9), elk ±15 dB.
   - LOW 50 Hz tot 1 kHz (standaard 100 Hz), MID 500 Hz tot 5 kHz (standaard 1 kHz), HIGH 2,5 kHz tot 15 kHz (standaard 8 kHz).
   - De engine krijgt frequentie en gain (niet de coëfficiënten) en rekent zelf op zijn eigen samplerate; een apparaatwissel vraagt dus niets van de UI.
   - Frequentie en gain worden per sample overgevloeid (zoals de fader nu), en de coëfficiënten worden elke 32 samples opnieuw berekend uit de overgevloeide waarden. Een band die op 0 dB staat en daar is aangekomen wordt overgeslagen, zodat een vlakke EQ bit-voor-bit doorgeeft.
5. **OL**-lampje: piek na de EQ boven -3 dBFS (handleiding: OL zit na de EQ), blijft 0,5 s aan.
6. Aftakking **DIRECT** (naar spoor N als DIRECT aan staat)
7. **Fader** en **MUTE** (MUTE is een DAW-gemak, de 388 heeft het niet)
8. Aftakkingen **EFF** en **AUX POST** (na de fader, voor de PAN)
9. **PAN**, constant power
10. **ASSIGN 1-2, 3-4, 5-6, 7-8**: het linkerdeel van de PAN naar de oneven bus, het rechterdeel naar de even bus. **L-R**: naar de stereobus.

### PGM-bussen en sporen

- Bussen 1, 3, 5 en 7 gaan door de **ODD**-master, 2, 4, 6 en 8 door de **EVEN**-master.
- Spoor N neemt **PGM-bus N** op (na de master), of met **DIRECT** aan het DIRECT-signaal van kanaal N.
- Opnemen vraagt geen ingang meer: elk spoor op REC FUNCTION neemt op wat er binnenkomt, ook zonder ingangsapparaat (ping-pong van band naar band). De REC-melding "wat ontbreekt" blijft alleen voor: geen projectmap, en VARI niet op 0%. Krijgt een klaargezet spoor niets binnen (geen DIRECT en niets op bus N, of DIRECT van een kanaal zonder bron), dan toont de UI bij REC een waarschuwing maar neemt wel op.
- **Latencycompensatie** geldt alleen voor live bronnen. Bij punch-in kijkt de engine of de opname van spoor N een live bron bevat (DIRECT van een live kanaal, of een live kanaal dat op bus N is geassigned). Zo ja, dan begint de take `compensatie` eerder op de band, zoals nu. Zo niet (bounce van band naar band), dan begint hij precies op de bandpositie, zodat een ping-pong sample-precies onder de bron ligt. Deze keuze valt één keer, bij de punch-in; ASSIGN, DIRECT of bron wijzigen tijdens de take verandert haar niet. Een bus die live en band mengt wordt gecompenseerd voor het live deel; het banddeel komt dan de apparaatlatency te vroeg (bekende beperking, in PROJECT.md).
- **Meeluisteren uit**: een live bron wordt wel opgenomen maar is nergens te horen (niet in monitor, stereobus, AUX of EFF). De engine houdt daarom per bus twee sommen bij: een voor opname (alles) en een voor wat je hoort (zonder live bronnen). Hetzelfde geldt voor DIRECT. Met meeluisteren aan zijn beide gelijk.

### MONITOR-sectie

Per spoor N een **MONITOR GAIN** (-inf tot +10 dB), **PAN** en **MUTE**. De bron van spoor N is "ingang" (de hoorbare som van PGM-bus N, of het DIRECT-signaal bij DIRECT) of de band, volgens de tabel uit de handleiding:

| MON | REC FUNCTION | INSERT | Transport / spoor | Bron |
|---|---|---|---|---|
| PGM | - | - | - | ingang |
| CUE | aan | aan | spoor neemt op | ingang |
| CUE | aan | aan | band speelt, spoor neemt niet op | band |
| CUE | aan | aan | band staat stil of spoelt | stil |
| CUE | aan | uit | - | ingang |
| CUE | uit | - | band speelt | band |
| CUE | uit | - | band staat stil of spoelt | stil |

- "Band speelt" is ook tijdens het aftellen niet waar (de band wacht).
- **Gewijzigd ten opzichte van nu**: een klaargezet spoor met INSERT aan hoort bij stilstaande band niets (vroeger de ingang), zoals op de 388. Wie dan het ingangsniveau wil horen zet MON op PGM.
- Staat er **ergens een kanaal op L-R**, dan gaat de MONITOR-mix niet naar de stereomaster (zie Stereo). De bronkeuze blijft gewoon doorwerken voor de TRK/PGM-meters.
- De **TRK/PGM-meter** van spoor N toont de bron uit deze tabel, voor MONITOR GAIN.

### Stereo, AUX en EFF

- **Stereobus** = de L-R-kanalen plus EFFECT RTN 1 en 2 (GAIN en PAN, stil tot fase 6). Staat geen enkel kanaal op L-R, dan komt de MONITOR-mix in plaats van de L-R-kanalen.
- Daarna **STEREO MASTER** en dan **MONO** (L en R gemiddeld op beide kanten). De stereometers meten na MONO.
- **AUX MASTER** en **EFF MASTER**: mono sommen van de sends.
- **Hoofduitgang** (kanaal 1 en 2, of de mono-uitgang): keuze **STEREO / AUX / EFF** (AUX en EFF mono op beide kanten), daarna het **MONITOR/PHONES**-niveau. De klik van de metronoom komt daarna, met zijn eigen volume, zodat PHONES de klik niet zachter zet. De klik blijft buiten alle meters en opnames, zoals nu.
- **AUX-uitgang** en **EFF-uitgang**: een interface-kanaal (vanaf 3) of geen. App-voorkeuren `aux_output` en `eff_output` in `settings.json`. De instellingen tonen alleen kanalen die het apparaat heeft; een opgeslagen kanaal dat het huidige apparaat niet heeft wordt overgeslagen, zonder melding.
- **METER**-schakelaar: de twee stereometers tonen L/R of AUX/EFF (AUX en EFF na hun master).
- De bestaande schakelaar TRACK METERS PRE/POST blijft voor de kanaalmeters: na de EQ en voor de fader, of na fader en mute. Kanaalmeters meten altijd het signaal zelf, ook met meeluisteren uit, zodat het ingangsniveau zichtbaar blijft bij direct monitoring via de interface. De TRK/PGM-meter van een opnemend spoor is dan stil (zoals nu).

## Engine

- `crates/engine/src/mixer.rs`
  - `ChannelParams` (Copy): bron, trim, EQ (drie keer freq en gain), aux, aux_pre, eff, assign, lr, direct, fader, pan, mute.
  - `Channel`: de DSP-toestand (filtergeheugen, overgevloeide waarden).
  - `Channel::process(input, live, sample_rate) -> Taps` met de aftakkingen: aux pre, direct, post-fader mono, eff, aux post, links en rechts na PAN, plus de pieken voor de meters en OL.
  - `Preamp` en `Eq` als losse, apart testbare stukjes.
- `crates/engine/src/monitor.rs`
  - `MonitorSource { Input, Tape, Silent }`.
  - `fn monitor_source(mon_cue, armed, insert, recording, tape_playing) -> MonitorSource`, als pure functie per tabelregel.
- `Engine::render` per frame:
  1. band lezen per spoor
  2. ingang lezen
  3. per kanaal de bron kiezen en `process`
  4. bussen sommeren (opname en hoorbaar)
  5. sporen voeden (opname)
  6. monitor en stereobus mixen
  7. AUX en EFF
  8. uitgangen vullen en de klik erbij
- Nieuwe commando's:
  - `SetChannel(usize, ChannelParams)`
  - `SetMonitor(usize, MonitorParams)` met gain, pan en mute
  - `SetMixer(MixerParams)` met ODD, EVEN, AUX- en EFF-master, RTN, MON, MONO, keuze hoofduitgang, phones, AUX- en EFF-uitgangskanaal

  `SetGain`, `SetPan`, `SetMute` en `SetInputChannel` gaan op in `SetChannel`. `SetMaster` blijft de STEREO MASTER. `SetArmed`, `SetInsert`, `SetMonitoring` en de transport- en metronoomcommando's blijven ongewijzigd.
- Meters terug via atomics: per kanaal pre en post (plus OL), per spoor TRK/PGM, stereo L en R, AUX en EFF.
- Geen allocaties, locks of I/O in de audiothread, zoals nu.

## Bediening

- **Kanaalstrook** van boven naar beneden:
  - naam met Laad/Wis
  - INPUT MIC/LINE/RMX met de ingangskeuze
  - TRIM (rood) met OL
  - HIGH, MID en LOW (concentrisch, oranje met grijze ring)
  - AUX (groen) met PRE/POST, daarna EFF (blauw)
  - ASSIGN 1-2/3-4/5-6/7-8 en L-R
  - PAN (crème knop)
  - REC FUNCTION, DIRECT en MUTE
  - fader met meter
- **Mastersectie**:
  - MONITOR: 8 keer GAIN, PAN en M, met een TRK/PGM-meter per spoor
  - PGM MASTERS ODD en EVEN
  - AUX MASTER en EFF MASTER (oranje)
  - EFFECT RTN 1 en 2
  - schakelaars MON PGM/CUE, METER L-R/AUX-EFF, MONO en STEREO/AUX/EFF
  - MONITOR/PHONES
  - STEREO MASTER (rood) met de stereometers
  - TRACK METERS PRE/POST
- Venster standaard ongeveer 1600 × 950; is het kleiner, dan scrolt de mixer ook verticaal.
- Alle knoppen: dubbelklik naar de standaardwaarde, waarde in een tooltip.
- Instellingen, tabblad Audio: AUX-uitgang en EFF-uitgang.
- De vintage-skin volgt in fase 8. Tot dan de huidige stijl met de kleuren uit de HANDOVER.

## Projectbestand

- `version` gaat van 1 naar 2. Een oudere DAW 388 weigert versie 2 al met een melding (`version > FORMAT_VERSION`).
- Per spoor erbij:
  - `channel`: `source` (mic/line/rmx), `trim_db`, `eq` (`low`, `mid` en `high`, elk met `freq_hz` en `gain_db`), `aux_db`, `aux_pre`, `eff_db`, `assign` (vier booleans), `lr`, `direct`
  - `monitor`: `gain_db`, `pan`, `mute`

  `fader_db`, `pan`, `mute` en `input` blijven waar ze staan (de kanaalinstellingen).
- Bovenaan erbij: `mixer` met `odd_db`, `even_db`, `aux_master_db`, `eff_master_db`, `effect_rtn` (twee keer `gain_db` en `pan`), `phones_db`, `mon` (pgm/cue), `meter` (lr/aux_eff), `mono` en `listen` (stereo/aux/eff). `master_db` blijft de STEREO MASTER.
- **Startstand van een nieuw project** (opnamestand):
  - elk kanaal RMX, geen ASSIGN, L-R uit, DIRECT uit
  - kanaalfaders op 0 dB, sends dicht (-inf), EQ vlak, TRIM 0 dB
  - MONITOR GAIN 0 dB, PAN midden
  - ODD, EVEN, AUX- en EFF-master en PHONES op 0 dB
  - MON op CUE, METER op L-R, listen op STEREO
- **Versie 1 openen**:
  - de oude `fader_db`, `pan` en `mute` gaan naar MONITOR GAIN, PAN en MUTE; de kanaalfader gaat naar 0 dB en PAN naar het midden
  - een spoor met een `input` wordt een LINE-kanaal met DIRECT aan, de rest blijft RMX
  - verder de startstand

  Afspelen klinkt dan als voorheen. Opnemen en meeluisteren werken ook als voorheen, op de INSERT-wijziging na (zie MONITOR-sectie).
- De omzetting gebeurt op `version == 1`, niet op ontbrekende velden, zodat serde-standaardwaarden haar niet ongemerkt overslaan. Pan loopt in beide versies van -1 tot 1.
- In de CHANGELOG vermelden: in een omgezet project staat de oude spoorfader in de MONITOR-sectie en de kanaalfader op 0 dB, dus TRACK METERS POST toont het oude faderniveau niet meer.

## Oplevering in drie stappen

Na elke stap test Ton; er wordt pas gecommit na "commit en push".

1. **0.8.0 Routering**: bron MIC/LINE/RMX (voorversterker al als schone gain, verzadiging in stap 2), TRIM, fader, PAN-knop, ASSIGN, L-R, PGM-bussen met ODD en EVEN, opnemen van bus N of DIRECT, latencycompensatie per bron, MONITOR-sectie met de tabel, MON PGM/CUE, MONO, TRK/PGM-meters, `project.json` versie 2 met de omzetting van versie 1.
   *Handmatig*: een bestaand project openen (klinkt en neemt op als voorheen), opnemen via een bus, opnemen via DIRECT, ping-pong van 1-3 naar spoor 4 (sample-precies), INSERT met MON op CUE en PGM, remix met L-R.
2. **0.9.0 Kanaalklank**: EQ met concentrische knoppen, MIC-verzadiging, OL-lampje.
   *Handmatig*: EQ op een drumspoor sweepen zonder tikken, MIC hard aansturen tot OL.
3. **0.11.0 Sends en masters** (0.10.0 werd de eenvoudige stand, zie `2026-10-28-eenvoudige-stand-design.md`): AUX PRE/POST, EFF, AUX- en EFF-masters naar interface-uitgangen, STEREO/AUX/EFF, MONITOR/PHONES, METER-schakelaar. EFFECT RTN gaat naar fase 6, samen met de effecten.
   *Handmatig*: koptelefoonmix op uitgang 3 met de fader dicht (PRE), EFF zakt mee met de fader, STEREO/AUX/EFF wisselen.

## Testen

Automatisch (engine en project):

- **Routering**
  - PAN naar oneven en even bus, ODD- en EVEN-master
  - spoor N neemt bus N op, DIRECT neemt kanaal N na EQ op, de fader telt bij DIRECT niet mee
  - RMX is stil zolang het spoor naar zijn ingang luistert, en speelt met INSERT tot de punch-in
  - opnemen zonder ingangsapparaat (bounce)
  - ping-pong: de take ligt sample-precies onder de bron (geen compensatie)
  - live opname: compensatie zoals nu
  - meeluisteren uit: live bron opgenomen maar niet hoorbaar, ook niet op AUX of EFF
- **MONITOR**
  - `monitor_source` regel voor regel, inclusief INSERT bij stilstand (stil) en het aftellen
  - L-R schakelt de monitor van de stereomaster af
- **Klank**
  - EQ: +15 en -15 dB op de middenfrequentie binnen 0,5 dB, vlak bij 0 dB (bit-voor-bit), sweepen zonder sprongen
  - voorversterker: LINE lineair, MIC verzadigt boven de drempel en blijft begrensd, OL gaat aan na EQ-boost
- **Sends**
  - AUX PRE onafhankelijk van fader en EQ, AUX POST en EFF volgen de fader en niet de PAN
  - PHONES schaalt de klik niet
- **Project**: omzetting van versie 1 naar 2 (fader naar MONITOR GAIN, ingang naar LINE met DIRECT), opslaan en opnieuw openen
- Alle bestaande tests van transport, opname en metronoom blijven groen, aangepast aan de nieuwe commando's

## Buiten deze fase

- Plate reverb en tape echo (fase 6), en dus geluid op EFFECT RTN.
- Ruis van de voorversterker (eventueel met het tape-model in fase 6).
- ACCESS SEND/RCV, PGM BUSS IN, STEREO BUSS IN, AUX BUSS IN en EFF BUSS IN (hardware-aansluitingen zonder nut in een DAW).
- Regio's en bewerken (fase 5). Ping-pong werkt na deze fase al via de bussen.
