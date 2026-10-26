# DAW 388

Een super eenvoudige 8-sporen DAW voor macOS en Windows, qua opzet gebaseerd op de Tascam 388 (Studio 8): een 8:8:2-mixer gecombineerd met een 8-sporen bandrecorder met tape-karakter.

Hobbyproject van Ton van der Kolk. Gratis te delen, niet te verkopen. Geen relatie met Tascam.

## Status

Versie 0.1.0: 8 WAV-bestanden laden en synchroon afspelen via een eenvoudige stereomix, met keuze van het audio-uitvoerapparaat. Zie [ROADMAP.md](ROADMAP.md) voor wat er volgt.

## Bouwen en starten

Vereist een recente stable Rust-toolchain (via [rustup](https://rustup.rs)).

```bash
cargo run --release
```

WAV-bestanden kun je ook meteen meegeven; ze komen op volgorde van naam op spoor 1 tot en met 8:

```bash
cargo run --release -- pad/naar/01_Kick.wav pad/naar/02_Snare.wav
```

Bediening: spatie is afspelen/stoppen, Home is RTZ (terug naar nul). Dubbelklik op een fader zet hem op 0 dB, dubbelklik op een panknop zet hem in het midden. Shift tijdens het slepen van een fader geeft fijne regeling.

## Opbouw

Zie [PROJECT.md](PROJECT.md).

## Licentie

Naar keuze [MIT](LICENSE-MIT) of [Apache 2.0](LICENSE-APACHE).
