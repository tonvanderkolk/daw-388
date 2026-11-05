# MIDI learn met terugmelding (0.37.0)

## Waarom

Live mixen met de muis gaat één fader tegelijk. Met een MIDI-controller heb je er meer onder je vingers. Dat past bij de manier van werken van de 388 en het mixdown deck: alles live, zonder automatisering. Ton gebruikt een Evolution MK-225C, een BCF2000 (motorfaders) en een X-Touch One. Anderen zullen een nanoKONTROL of iets dergelijks hebben.

0.37.0 is MIDI learn met terugmelding. Dat werkt met elke controller die CC of noten stuurt. Mackie Control (de X-Touch One, de BCF2000 in MCU-stand) volgt in 0.38.0 en bouwt hierop voort.

## Wat je kunt koppelen

- Per spoor: de fader, PAN, MUTE en SOLO.
- De STEREO MASTER.
- De transporttoetsen PLAY, STOP, REC en RTZ.

AUX, EFF, EQ, TRIM, de effectknoppen en de MONITOR-sectie blijven buiten deze versie. Ze kunnen er later met dezelfde techniek bij.

## Verbinding

- De app luistert naar alle MIDI-ingangen tegelijk (crate `midir`, CoreMIDI op macOS, WinMM op Windows). Er valt niets te kiezen. Een controller die later wordt ingeplugd, vindt de app vanzelf: hij kijkt elke paar seconden of er poorten bij zijn gekomen of zijn verdwenen.
- Een koppeling onthoudt de naam van de poort, het MIDI-kanaal, het soort bericht (CC of noot) en het nummer. Twee controllers met dezelfde CC zitten elkaar dus niet in de weg.
- De terugmelding gaat naar de uitgang met dezelfde naam als de ingang. Is die er niet, dan valt de terugmelding weg.

## Leren en wissen

- Rechtsklik op een element dat je kunt koppelen. Het menu heeft dan **MIDI Learn**. Het element knippert en de statusregel zegt "Move a control on your MIDI controller (Esc cancels)". Het eerste bericht dat binnenkomt, wordt de koppeling. Esc of een tweede rechtsklik breekt het leren af.
- Zat dat bericht al aan een ander element, dan gaat het daar weg: één bericht stuurt één element.
- Een gekoppeld element krijgt een klein stipje. Het menu heeft dan ook **Forget MIDI**.
- In Settings komt een deel **MIDI** met de poorten die er zijn en een lijst van de koppelingen ("Fader 3 ← BCF2000, ch 1, CC 83"), elk met een knop om hem te wissen. Een knop **Forget all** wist ze allemaal.
- De koppelingen horen bij de app (`settings.json`) en niet bij het project: je controller blijft dezelfde als je een ander nummer opent.

## Gedrag

- **Faders:** CC 0 tot 127 over de hele weg van de fader, op dezelfde kromme als met de muis. 0 dB ligt dus op dezelfde plek als op het scherm. Een noot op een fader doet niets.
- **PAN:** CC 0 is links, 64 het midden, 127 rechts.
- **Pickup:** staat de controller ergens anders dan het element op het scherm, dan gebeurt er niets tot de controller het element passeert of er binnen 2 stappen van komt. Daarna volgt het element de controller. Dat voorkomt sprongen. Een verandering op het scherm (de muis, undo, een project openen) maakt de koppeling weer wachtend op pickup.
- **Terugmelding:** elke verandering van een gekoppeld element gaat als CC terug, ook door undo of het openen van een project. Daardoor bewegen de motorfaders van de BCF2000 vanzelf mee. Pickup is daar dan meteen voldaan. Voor een controller zonder motoren is het onschadelijk. Waarden die van de controller zelf komen, gaan niet terug, anders gaat de motorfader schokken.
- **Toetsen:** een noot aan (snelheid boven 0) of een CC boven 63 telt als indrukken; loslaten doet niets. MUTE en SOLO wisselen bij elk indrukken. PLAY, STOP, REC en RTZ doen wat die toets op het scherm doet. De terugmelding van MUTE en SOLO is 127 voor aan en 0 voor uit, zodat lampjes op de controller meedoen.
- **LINK:** op een gekoppeld paar beweegt de andere kant mee, zoals nu met de muis.
- **De weergave:** in de compacte console en in TRACKS werken de koppelingen gewoon door; leren kan alleen waar het element te zien is.

## Techniek

- Een nieuw module `crates/ui/src/midi.rs` bevat:
  - de typen `Control` (wat er te koppelen is), `Source` (poort, kanaal, CC of noot, nummer) en `Binding`;
  - het leren, pickup en de vertaling van een bericht naar een `Control` met een waarde;
  - de terugmelding: per frame vergelijkt het de waarde van elk gekoppeld element met wat het het laatst stuurde.
- Het platform (`Platform`) krijgt `midi_inputs()`, een kanaal van binnenkomende berichten en `send_midi(port, bytes)`. `app-desktop` doet dat met `midir`; de `FakePlatform` van de tests niet. De tests sturen nep-berichten.
- MIDI komt binnen op de thread van `midir`. Via een kanaal pakt de app de berichten elke frame op en vraagt een repaint aan. De engine verandert niet: de app zet de fader zoals de muis dat doet.

## Tests

- Een CC op een gekoppelde fader zet de fader op de juiste dB (0, 64, 127 en de plek van 0 dB).
- Pickup: een CC ver van de fader doet niets; een CC die hem passeert pakt hem op; daarna volgt hij.
- Leren: het eerste bericht koppelt; een bericht dat al aan iets anders zat, gaat daar weg; Esc breekt af.
- Toetsen: noot aan en CC 127 wisselen MUTE; CC 0 en noot uit doen niets; PLAY start het transport.
- De terugmelding: een fader die met de muis of undo verandert, stuurt één CC; een waarde van de controller zelf gaat niet terug.
- LINK: een CC op de linkerkant van een gekoppeld paar zet beide.
- De koppelingen overleven opslaan en laden van `settings.json`; een oud bestand zonder koppelingen laadt.

## Buiten de scope

- Mackie Control (de X-Touch One, de BCF2000 in MCU-stand): 0.38.0.
- AUX, EFF, EQ, TRIM, de effecten en de MONITOR-sectie.
- 14-bit CC, pitch bend en NRPN.
- Koppelingen per project en presets per controller.

## Documentatie

- De handleiding krijgt een hoofdstuk MIDI controllers: leren, wissen, pickup, motorfaders en de lijst in Settings.
- CHANGELOG, ROADMAP, PROJECT.md (het module en de instellingen), CLAUDE.md en de versie in Cargo.toml naar 0.37.0.
