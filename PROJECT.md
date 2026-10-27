# PROJECT - DAW 388

Versie: 0.7.0
Laatst bijgewerkt: 26-10-2026

## Doel

Een eenvoudige standalone 8-sporen DAW naar het model van de Tascam 388: opnemen via een audio-interface of de ingebouwde audio, mixen op een 8:8:2-mixer met tape-karakter en exporteren als stereo bounce of losse sporen. De volledige functionele scope en de genomen besluiten staan in [HANDOVER.md](HANDOVER.md).

## Tech stack

- Rust (stable, edition 2024), Cargo workspace
- Audio I/O: `cpal` 0.18 (CoreAudio op macOS, WASAPI op Windows)
- UI: `egui` / `eframe` 0.37
- Audiobestanden: `hound` (WAV) en `symphonia` (AIFF en AIFF-C)
- Realtime-communicatie: `rtrb` (lock-free ringbuffer)
- Bestandsdialogen: `rfd`
- Builds: GitHub Actions voor macOS en Windows

## Workspace

| Crate | Map | Taak |
|---|---|---|
| `daw388-engine` | `crates/engine` | Transport, 8 sporen en stereomix. Platformonafhankelijk, geen allocaties of locks in de audiothread |
| `daw388-tape` | `crates/tape` | Tape-emulatie en dbx (fase 6, nog leeg) |
| `daw388-fx` | `crates/fx` | Plate reverb en tape echo (fase 6, nog leeg) |
| `daw388-project` | `crates/project` | WAV- en AIFF-import naar mono, samplerate-conversie en de projectmap (`project.json`, `audio/`, `backups/`). Later export |
| `daw388-ui` | `crates/ui` | egui-interface, praat met het platform via de `Platform`-trait |
| `daw388` | `crates/app-desktop` | Desktop-entrypoint: cpal-uitvoer, bestandsdialogen en het eframe-venster |

## Architectuur

- De UI-thread bezit een `EngineHandle`, de audio-callback bezit de `Engine`.
- Commando's (PLAY, STOP, locate, spoor laden, gain, pan, mute, master) gaan via een lock-free ringbuffer naar de audiothread.
- Positie, afspeelstatus en piekmeters komen terug via atomics. Per spoor meet de engine twee pieken: voor de fader (het niveau van het spoor zelf, zoals de spoormeters van de 388) en na fader en mute. De schakelaar TRACK METERS PRE/POST kiest welke de UI toont.
- Sporenbuffers die de engine loslaat gaan via een tweede ringbuffer terug naar de UI-thread en worden daar vrijgegeven. Er wordt dus nooit geheugen vrijgegeven in de audiothread.
- De engine kent geen audioapparaten. `app-desktop` roept `Engine::render` aan vanuit de cpal-callback; een latere browserversie doet hetzelfde vanuit WebAudio.
- Bij het wisselen van apparaat bouwt de app een nieuwe engine op de samplerate van het apparaat en stuurt de sporen (opnieuw geconverteerd), mixinstellingen en positie opnieuw.
- Sporen zijn mono, zoals op de 388. Stereobestanden worden bij het laden gemiddeld naar mono.
- Het formaat van een audiobestand wordt herkend aan de eerste bytes (`RIFF` of `FORM`), niet aan de extensie.

## Projectmap

```
Mijn liedje/
├── project.json        # mixinstellingen en verwijzingen naar de audio
├── audio/              # WAV-bestanden van de sporen
└── backups/            # eerdere versies van project.json (laatste 10)
```

- `project.json` bevat `format` (`daw388-project`), `version` (formaatversie, nu 1), `app_version`, `master_db`, `meter_post` (spoormeters na de fader, standaard `false`), `transport` (zie hierboven) en per spoor `number`, `file` (relatief pad), `fader_db`, `pan` en `mute`.
- Opslaan kopieert audio van buiten het project naar `audio/`. Het origineel blijft staan. Bij een naamconflict krijgt de kopie een achtervoegsel (`take-2.wav`).
- `project.json` wordt eerst naar een tijdelijk bestand geschreven en daarna hernoemd, zodat een crash nooit een half bestand achterlaat.
- Autosave volgt 30 seconden na een wijziging, alleen als het project al een map heeft.
- Backups: bij elke handmatige opslag en hooguit elke 5 minuten bij autosave.
- Een project uit een nieuwere formaatversie wordt geweigerd met een melding.

## SoundCraft

[SoundCraft](https://github.com/storytold/soundcraft) (MIT OR Apache-2.0) is bekeken als referentie. Het is een groot project (ruim 110.000 regels) dat vastzit aan eigen forks van egui en winit en aan eigen hulpbibliotheken, dus er is geen code rechtstreeks overgenomen. Wel gevolgd:

- Het patroon UI-handle tegenover audio-callback met commandokanaal en atomics.
- Apparaat zoeken op naam met terugval op het standaardapparaat (`app-desktop/src/main.rs`, met bronvermelding).
- Een scratchbuffer per callback zodat de callback niet alloceert, met omzetting naar het sampleformaat van het apparaat.

## Transport

Naar de handleiding van de 388 (Memory/Transport Controls):

- De bandpositie loopt in samples op de samplerate van de engine, met een fractie, zodat de pitch-knop de snelheid kan variëren (plus of min 15%, VARI/FIX). Tussen samples wordt met Hermite-interpolatie gelezen. De teller volgt de bandpositie, zoals de 388 de bandlengte meet.
- RESET zet het nulpunt op de huidige plek; de teller toont de tijd vanaf het nulpunt, met een minteken daarvoor.
- CUE onthoudt de huidige plek. STC zoekt het CUE-punt op (zonder CUE-punt: het begin). RTZ zoekt het nulpunt op. PLAY tijdens een zoekactie speelt bij aankomst.
- REW en F FWD spoelen op 30 keer de normale snelheid, stil, tot STOP, het begin of het einde van het langste spoor.
- REPEAT loopt van het nulpunt tot het CUE-punt. In de 388-modus spoelt de band terug en speelt opnieuw, en zet elke transporttoets behalve PLAY en RTZ REPEAT uit. In de cycle-modus springt de positie direct terug (ook bij RTZ en STC) en blijft REPEAT aan.
- Nulpunt, CUE-punt, REPEAT-modus, pitch en VARI/FIX worden in `project.json` bewaard onder `transport`.

## Opnemen

Naar de handleiding van de 388 (REC FUNCTION, REC, INSERT, Punch-in):

- REC FUNCTION maakt een spoor klaar (lampje knippert); tijdens de opname brandt het. REC start de opname op de klaargezette sporen, vanuit stop samen met PLAY. Zonder klaargezette sporen gaat de machine record-ready (REC knippert) en start REC FUNCTION de opname op dat spoor; nogmaals drukken punch-out.
- Punch-out met PLAY (speelt door), STOP, REW, F FWD, RTZ of STC, of automatisch bij het CUE-punt (de band stopt, of spoelt terug met REPEAT).
- INSERT: klaargezette sporen spelen de band tot de punch-in en schakelen dan naar de ingang. Zonder INSERT hoort een klaargezet spoor altijd de ingang. Meeluisteren via DAW 388 kan uit (direct monitoring van de interface); een spoor dat opneemt speelt dan niets.
- AUTO PUNCH: na REC wordt alleen tussen nulpunt en CUE-punt opgenomen, bij elke doorgang met REPEAT opnieuw.
- De ingang draait op de samplerate van de uitvoer. De input-callback vult een lock-free ring, de output-callback leest per uitvoerframe een invoerframe, zodat band en ingang op dezelfde klok lopen. Bij twee apparaten met een eigen klok stuurt de engine bij door af en toe een frame over te slaan.
- Latencycompensatie: de tijd van opname tot callback (ingang) plus van callback tot weergave (uitgang), gemeten met de tijdstempels van cpal, plus de vulling van de ring, plus een handmatige correctie in milliseconden. Een opname wordt zoveel eerder op de band gelegd.
- De engine stuurt per spoor een stroom `Start(positie)`, samples en `Stop` naar een schrijver op een eigen thread, die elke take als 32-bit float WAV in een tijdelijke map zet. Daarna legt een achtergrondtaak de take over het spoor (alleen het opgenomen stuk wordt vervangen, zoals op band) en schrijft het resultaat als 24-bit WAV `NN_Naam_opnameN.wav` in `audio/`. Takes van hetzelfde spoor worden op volgorde verwerkt.
- Cmd+Z (Bewerken, Opname ongedaan maken) zet tot 10 opnames terug; de bestanden blijven in `audio/` staan.
- Opnemen gaat op normale snelheid; met de pitch-knop op VARI en niet op 0% weigert REC met een melding.
- Een project zonder map vraagt bij REC eerst om op te slaan.
- Ingangsapparaat, meeluisteren en latencycorrectie zijn app-voorkeuren; de ingang per spoor, INSERT en AUTO PUNCH worden per project bewaard.

## Metronoom

- De klik wordt in de engine gemaakt (een korte sinus met snelle uitsterving, hoger op tel 1) en gaat alleen naar de uitvoer: niet in de mixmeters en nooit in een opname.
- Maat 1 begint aan het begin van de band, zodat RESET het ritme niet verschuift. Het tempo telt kwartnoten per minuut, ook in x/8; een tel is dan een achtste.
- De klik klinkt tijdens opnemen en aftellen, en tijdens gewoon afspelen alleen als dat is aangezet.
- Aftellen (1 of 2 maten) gebeurt bij REC vanuit stop. De band wacht; het maatraster loopt voor de band uit, zodat de klik na het aftellen zonder sprong doorloopt en de opname begint waar de band stond.
- Tempo, maatsoort, CLICK, volume, aftellen en "ook bij afspelen" worden per project bewaard onder `metronome`.

## Laden

Audio wordt op een achtergrondthread gedecodeerd en naar de samplerate van het apparaat omgezet (via `Platform::spawn`). Elk verzoek krijgt een volgnummer; een resultaat voor een spoor dat intussen gewist of opnieuw geladen is, wordt genegeerd. Bij een apparaatwissel worden de sporen op de achtergrond opnieuw omgezet.

## Audio-uitvoer

- De gekozen uitvoer (apparaat, samplerate, buffergrootte) is een app-voorkeur, geen projectinstelling. Ze staat in `settings.json` in de configuratiemap van het systeem (macOS: `~/Library/Application Support/DAW 388/`, Windows: `%APPDATA%\DAW 388\`).
- Openen gaat in deze volgorde: het gekozen apparaat, anders het standaardapparaat (met een melding), anders de stille klok. De stille klok laat de engine in echte tijd draaien zonder geluid, zodat transport, teller en meters blijven werken.
- Meldt cpal dat het apparaat weg is (`DeviceNotAvailable` of `StreamInvalidated`), dan opent de app meteen opnieuw volgens die volgorde, op dezelfde bandpositie. Zolang de uitvoer niet het gekozen apparaat is, kijkt de app elke 2 seconden of het terug is en schakelt dan terug.
- Een samplerate die het apparaat niet biedt, of een buffergrootte buiten zijn bereik, valt terug op de standaard van het apparaat.

## Bekende beperkingen

- Import van WAV en AIFF (ongecomprimeerd). Nog geen FLAC, MP3 of andere formaten.
- Samplerate-conversie gebruikt Hermite-interpolatie: prima voor afluisteren, voor export komt een betere resampler.
- Geen opname, EQ, sends, effecten of tape-model. Zie de roadmap.
- Op macOS vraagt het systeem bij het eerste gebruik van een ingang om toegang tot de microfoon. Zolang de app als los programma vanuit de terminal draait, hoort die toestemming bij de terminal.
- Op macOS is het standaard app-menu uitgezet, zodat Cmd+Q via de opslaan-vraag loopt. Afsluiten door uitloggen of herstarten van de Mac gaat nog wel buiten die vraag om; autosave beperkt dan het verlies tot de laatste 30 seconden.

## Versiebeheer

- GitHub: privé repo `tonvanderkolk/daw-388`
- `design/ref/` (handleiding en foto's) staat in `.gitignore` en hoort niet in de repo.
- Wachtwoorden en tokens horen in `CREDENTIALS.md` (staat in `.gitignore`).
