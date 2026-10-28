# Ontwerp: eenvoudige stand (0.10.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek, wacht op beoordeling door Ton

## Doel

De 388-routering (bussen, ASSIGN, MONITOR-sectie) is trouw aan het origineel maar te ingewikkeld voor wie gewend is aan Logic en vergelijkbare DAW's. Een eenvoudige stand geeft per spoor één strook: ingang kiezen, REC, spelen; de fader en PAN zijn wat je hoort. De 388-stand blijft volledig bestaan. De klank (TRIM, voorversterker, EQ en straks het tape-model) is in beide standen gelijk.

## Besluiten

| Vraag | Besluit |
|---|---|
| Startstand van een nieuw project | Eenvoudig |
| Van 388 terug naar Eenvoudig | Na een vraag: de 388-routering wordt teruggezet. Geen verborgen instellingen |
| Pingpong in de eenvoudige stand | Niet. Daarvoor schakel je naar 388. De stereo-bounce van de mastermix komt in fase 7 voor beide standen |
| Bronkeuze in de eenvoudige stand | Ingang (of geen) plus MIC of LINE |
| Aanpak | Een AUTO-bron in de engine; de eenvoudige stand is een vaste instelling van de bestaande mixer plus een eenvoudiger weergave |

## Begrippen

- **AUTO**: een kanaal dat de band speelt, en de ingang zodra zijn spoor naar de ingang luistert (`hears_input`, zie de mixerspec: op REC FUNCTION en opnemend, of op REC FUNCTION met INSERT uit).
- **Eenvoudige instelling van een kanaal**: AUTO aan, DIRECT aan, L-R aan, geen ASSIGN.

## Engine

### AUTO

- `ChannelParams` krijgt `auto: bool`. `source` blijft MIC, LINE of RMX; bij AUTO bepaalt `source` alleen of de ingang door de voorversterker gaat (MIC) of schoon is (LINE). RMX met AUTO gedraagt zich als LINE.
- Per frame kiest een AUTO-kanaal:
  - **de ingang** als het spoor naar zijn ingang luistert en het kanaal een live bron heeft (gekozen ingang die het apparaat heeft)
  - **anders de band** van het spoor met hetzelfde nummer (stil als de band niet speelt)
- Een AUTO-kanaal zonder ingang speelt altijd de band; op REC neemt het dan stilte op, met de bestaande waarschuwing in de UI.

### Signaalvolgorde van een AUTO-kanaal

- **Ingang**: TRIM, voorversterker (bij MIC), **DIRECT-aftakking**, EQ, fader, PAN.
- **Band**: EQ, fader, PAN. TRIM en de voorversterker doen niets.

Zo komt de EQ niet op de opname en zit hij bij het afspelen er maar één keer op, zoals in Logic. TRIM is in AUTO een ingangsniveau. Een kanaal zonder AUTO houdt de volgorde uit de mixerspec (DIRECT na de EQ, TRIM ook op RMX).

Bij AUTO is `Taps::direct` de waarde voor de EQ. Het OL-lampje en de PRE-meter blijven na de EQ meten; `Taps` krijgt daarvoor een veld `pre` (na de EQ, voor de fader), en de PRE-meter en OL lezen `pre` in plaats van `direct`. Bij een kanaal zonder AUTO zijn `pre` en `direct` gelijk.

### Overgang tussen band en ingang

Het wisselen gebeurt op de sample waarop `hears_input` verandert, dus bij INSERT precies op de punch-in en de punch-out. Er komt geen overvloeiing: de band en de ingang zijn op dat punt bedoeld om aan te sluiten, net als op een bandrecorder. EQ, fader en PAN lopen gewoon door, zodat er geen sprong in de instellingen is.

### Latencycompensatie

`feeds_live` telt een AUTO-kanaal met een live bron mee als live bron, net als MIC en LINE nu. Een bounce bestaat in de eenvoudige stand niet.

### Meeluisteren uit

Zoals nu: een live bron wordt opgenomen maar niet gehoord. Bij AUTO is het kanaal dan stil zolang het de ingang kiest.

## Projectbestand

- `mixer.mode`: `simple` of `388`. Ontbreekt het veld, dan `388`.
- `channel.auto`: `bool`, standaard `false`.
- De formaatversie blijft 2.
- **Nieuw project**: `mode` = `simple`, elk kanaal met de eenvoudige instelling, bron LINE zonder ingang, EQ vlak, TRIM, fader en MONITOR op 0 dB.
- **Project van 0.8.0 of 0.9.0** (versie 2 zonder `mode`): 388-stand, ongewijzigd.
- **Versie 1 (0.7.0 en ouder)**: wordt een eenvoudig project. De oude `fader_db`, `pan` en `mute` blijven de kanaalfader, PAN en MUTE (niet langer naar de MONITOR-sectie). Elk kanaal krijgt de eenvoudige instelling met bron LINE; een spoor met een `input` houdt die ingang. Dit vervangt de omzetting naar LINE plus DIRECT en de MONITOR-sectie uit 0.8.0.

## Bediening

### Schakelaar

In de bovenbalk naast de projectnaam: **Eenvoudig | 388**.

- **Eenvoudig naar 388**: geen wijziging in wat de engine krijgt. De 388-strook toont AUTO als vierde bron naast MIC, LINE en RMX (een afwijking van de 388, zoals DIRECT).
- **388 naar Eenvoudig**: staat er iets anders dan de eenvoudige instelling (een kanaal zonder AUTO, DIRECT of L-R, een ASSIGN, of een MONITOR-, ODD- of EVEN-waarde die niet de standaard is), dan vraagt de app eerst: "De bussen, ASSIGN en de MONITOR-instellingen gaan terug naar de eenvoudige stand. Doorgaan?" Bij ja krijgt elk kanaal de eenvoudige instelling; MIC blijft MIC, LINE blijft LINE en RMX wordt LINE. Fader, PAN, MUTE, EQ, TRIM en de ingang blijven. MONITOR, ODD, EVEN en MON gaan naar hun startstand. Bij nee blijft alles in de 388-stand.
- Een wissel is een gewone projectwijziging: hij markeert het project als gewijzigd en gaat mee in autosave.

### Kanaalstrook in de eenvoudige stand

Van boven naar beneden: nummer, naam, Laad en Wis; ingangskeuze met MIC/LINE ernaast; TRIM met OL (tooltip: TRIM regelt alleen de ingang); EQ (LOW, MID, HIGH) en PAN; REC en MUTE; fader met meter. Verborgen: RMX, AUTO, ASSIGN, L-R en DIRECT.

### Mastersectie in de eenvoudige stand

Verborgen: PGM MASTERS, de MONITOR-sectie en MON CUE/PGM. Zichtbaar: STEREO MASTER, MONO, TRACK METERS PRE/POST en de transportsectie.

### Overig

- De EQ-knoppen worden groter (ongeveer 34 pt) en de kanaalstrook breder waar nodig; in de eenvoudige stand is daar ruimte voor.
- PROJECT.md krijgt bij "Routing in de praktijk" een stukje over de eenvoudige stand en de schakelaar.

## Testen

- **Engine**
  - AUTO speelt de band zolang het spoor niet naar zijn ingang luistert, en de ingang zodra het dat wel doet
  - met INSERT wisselt AUTO precies op de punch-in en punch-out
  - de opname van een AUTO-kanaal heeft geen EQ; het afspelen wel
  - bij het afspelen van de band werken TRIM en de voorversterker niet
  - AUTO zonder ingang speelt de band en neemt stilte op
  - een AUTO-kanaal met live bron krijgt latencycompensatie
  - OL en de PRE-meter meten na de EQ, ook bij AUTO
- **Project**
  - versie 1 wordt een eenvoudig project met de oude fader op de kanaalfader
  - versie 2 zonder `mode` wordt 388
  - een nieuw project is eenvoudig
  - opslaan en opnieuw openen bewaart `mode` en `auto`
- **UI**
  - van 388 naar Eenvoudig zet de routering terug en laat fader, PAN, EQ, TRIM en ingang staan
  - van 388 naar Eenvoudig zonder afwijkende routering vraagt niets
  - van Eenvoudig naar 388 stuurt dezelfde kanaalinstellingen naar de engine
- Alle bestaande tests blijven groen. De test voor de omzetting van versie 1 verandert mee.

## Versies

Dit wordt **0.10.0**. De sends en masters (AUX, EFF, STEREO/AUX/EFF, MONITOR/PHONES, METER) schuiven op naar **0.11.0**; in de mixerspec en de ROADMAP aanpassen.

## Buiten dit ontwerp

- Pingpong of bounce naar een spoor in de eenvoudige stand.
- Stereo-bounce van de mastermix (fase 7, voor beide standen).
- Sends en effecten in de eenvoudige stand: dat bepalen we bij 0.11.0 en fase 6.
