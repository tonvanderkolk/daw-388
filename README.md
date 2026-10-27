# DAW 388

Een super eenvoudige 8-sporen DAW voor macOS en Windows, qua opzet gebaseerd op de Tascam 388 (Studio 8): een 8:8:2-mixer gecombineerd met een 8-sporen bandrecorder met tape-karakter.

Hobbyproject van Ton van der Kolk. Gratis te delen, niet te verkopen. Geen relatie met Tascam.

## Status

Versie 0.2.1: 8 WAV- of AIFF-bestanden laden (8 tot 32 bit, float, elke samplerate) en synchroon afspelen via een eenvoudige stereomix, met keuze van het audio-uitvoerapparaat. Projecten opslaan en openen, met autosave en backups. Zie [ROADMAP.md](ROADMAP.md) voor wat er volgt.

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

Bediening: Cmd+N, Cmd+O, Cmd+S, Shift+Cmd+S, Cmd+W en Cmd+Q voor nieuw, openen, opslaan, opslaan als, project sluiten en stoppen (Ctrl op Windows). Spatie is afspelen/stoppen, Home is RTZ (terug naar het nulpunt), komma is REW, punt is F FWD en C zet REPEAT aan of uit. Cmd+, opent de instellingen. Dubbelklik op een fader zet hem op 0 dB, dubbelklik op een panknop zet hem in het midden. Shift tijdens het slepen van een fader geeft fijne regeling.

## Opbouw

Zie [PROJECT.md](PROJECT.md).

## Licentie

Naar keuze [MIT](LICENSE-MIT) of [Apache 2.0](LICENSE-APACHE).
