# PROJECT - DAW 388

Versie: 0.3.0
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

- `project.json` bevat `format` (`daw388-project`), `version` (formaatversie, nu 1), `app_version`, `master_db`, `meter_post` (spoormeters na de fader, standaard `false`) en per spoor `number`, `file` (relatief pad), `fader_db`, `pan` en `mute`.
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

## Bekende beperkingen van 0.1.0

- Import van WAV en AIFF (ongecomprimeerd). Nog geen FLAC, MP3 of andere formaten.
- Laden en samplerate-conversie gebeuren op de UI-thread; bij lange bestanden hapert het venster even.
- Samplerate-conversie gebruikt Hermite-interpolatie: prima voor afluisteren, voor export komt een betere resampler.
- Geen opname, EQ, sends, effecten of tape-model. Zie de roadmap.
- Op macOS is het standaard app-menu uitgezet, zodat Cmd+Q via de opslaan-vraag loopt. Afsluiten door uitloggen of herstarten van de Mac gaat nog wel buiten die vraag om; autosave beperkt dan het verlies tot de laatste 30 seconden.
- Geen terugval naar een stille klok als er geen audioapparaat is; de app meldt dat dan bovenin.

## Versiebeheer

- GitHub: privé repo `tonvanderkolk/daw-388`
- `design/ref/` (handleiding en foto's) staat in `.gitignore` en hoort niet in de repo.
- Wachtwoorden en tokens horen in `CREDENTIALS.md` (staat in `.gitignore`).
