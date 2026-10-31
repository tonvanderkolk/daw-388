# DAW 388

A very simple 8-track DAW for macOS and Windows, modelled on the Tascam 388 (Studio 8): an 8:8:2 mixer combined with an 8-track tape recorder with tape character. Load or record eight tracks, mix them on a vintage console with EQ, AUX and EFF sends, a plate reverb and a tape echo, and export a stereo bounce or separate tracks. The interface is in English and uses the names of the 388 (TRK, PGM, AUX, REC, RTZ, CUE, ...).

A hobby project by Ton van der Kolk. Free and open source. Not affiliated with Tascam. The rest of the documentation is in Dutch.

Like DAW 388? You can [support it with a coffee](https://buymeacoffee.com/tvdk71).

### Download

Get the latest version from [Releases](https://github.com/tonvanderkolk/daw-388/releases).

- **macOS** (Apple Silicon and Intel, macOS 11 or later): the first time takes one extra step.
   1. Open the DMG and drag DAW 388 to Applications.
   2. Start DAW 388. The app is not signed with an Apple Developer ID, so macOS says it could not verify it. Click **Done**, not "Move to Trash".
   3. Open System Settings, Privacy & Security, scroll down and click **Open Anyway** next to DAW 388. Confirm with your password.
   4. From then on DAW 388 opens normally.

   Comfortable with Terminal? This one command does the same and skips the warning:

   ```bash
   xattr -dr com.apple.quarantine "/Applications/DAW 388.app"
   ```

- **Windows** (64-bit): unzip and start `DAW 388.exe`. If SmartScreen warns about an unknown publisher, choose "More info", then "Run anyway".

The **user manual** (PDF) comes with every release, inside the DMG and the ZIP, and opens from Help, User manual in the app.

Or build it yourself with `cargo run --release` (a recent stable Rust toolchain via [rustup](https://rustup.rs)).

---

Een super eenvoudige 8-sporen DAW voor macOS en Windows, qua opzet gebaseerd op de Tascam 388 (Studio 8): een 8:8:2-mixer gecombineerd met een 8-sporen bandrecorder met tape-karakter.

Hobbyproject van Ton van der Kolk. Gratis en open source. Geen relatie met Tascam.

Vind je DAW 388 fijn? Je kunt het [steunen met een koffie](https://buymeacoffee.com/tvdk71).

## Downloaden

De nieuwste versie staat bij [Releases](https://github.com/tonvanderkolk/daw-388/releases): een DMG voor macOS (Apple Silicon en Intel) en een ZIP voor Windows. De apps zijn niet ondertekend met een Apple Developer ID; zie hierboven (Download) hoe je ze de eerste keer opent.

## Status

Versie 0.30.0. Sinds 0.16.0 kwamen er de vintage console naar de 388 (VU-meters, bandrecorder, toetsen, vensters), de weergaven MIXER en SPOREN, opnemen op VARI, projectpakketten `.daw388`, downloadbare apps, een Help-menu met een korte gids, het importeren van stereobestanden over twee sporen en een compacte console voor lage schermen, een gebruikershandleiding als PDF, M, S en R in de sporen en het opruimen van ongebruikte opnames bij. Daarvoor al: 8 sporen laden (WAV, AIFF, FLAC en MP3) en afspelen, transport naar het voorbeeld van de 388, opnemen met REC FUNCTION, punch-in en AUTO PUNCH, een metronoom, een mixer met EQ, AUX- en EFF-sends, een plate reverb, een tape echo en het tape-karakter van de 388 met dbx bij afspelen in een eenvoudige stand (één strook per spoor) of met de volledige 388-routering (ASSIGN naar de PGM-bussen, DIRECT en een MONITOR-sectie), export van een stereo bounce en losse sporen als WAV of AIFF, projecten met autosave en backups, en keuze van audio-interface, samplerate en buffer. Hoe de routing werkt staat in [PROJECT.md](PROJECT.md#routing-in-de-praktijk). Zie [ROADMAP.md](ROADMAP.md) voor wat er volgt.

## Bouwen en starten

Vereist een recente stable Rust-toolchain (via [rustup](https://rustup.rs)).

```bash
cargo run --release
```

Audiobestanden kun je ook meteen meegeven; ze komen op volgorde van naam op spoor 1 tot en met 8:

```bash
cargo run --release -- pad/naar/01_Kick.wav pad/naar/02_Snare.wav
```

Een projectmap open je op dezelfde manier:

```bash
cargo run --release -- "pad/naar/Mijn liedje"
```

Bediening: Cmd+N, Cmd+O, Cmd+S, Shift+Cmd+S, Cmd+W en Cmd+Q voor nieuw, openen, opslaan, opslaan als, project sluiten en stoppen (Ctrl op Windows). Spatie is afspelen/stoppen, Home is RTZ (terug naar het nulpunt), komma is REW, punt is F FWD, C zet REPEAT aan of uit, R is REC en K zet de metronoom aan of uit. Cmd+Z en Shift+Cmd+Z zijn undo en redo. Cmd+, opent de instellingen. In SPOREN kopieert Alt en slepen een regio, Cmd tijdens het slepen schakelt het vastklikken uit en Cmd+Links en Cmd+Rechts zoomen. Help, Quick guide zet de basis op een rij. Dubbelklik op een fader zet hem op 0 dB, dubbelklik op een panknop zet hem in het midden. Shift tijdens het slepen van een fader geeft fijne regeling.

## Opbouw

Zie [PROJECT.md](PROJECT.md).

## Licentie

Naar keuze [MIT](LICENSE-MIT) of [Apache 2.0](LICENSE-APACHE).

Het lettertype IBM Plex staat onder de SIL Open Font License (`crates/ui/assets/fonts/OFL.txt`).
