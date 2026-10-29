# Ontwerp fase 8, stap 4: kopregel, vensters en displays (0.22.0)

Datum: 29-10-2026
Status: concept, ter goedkeuring

## Doel

De laatste stap van de vintage skin. De console is sinds 0.21.0 rustig en strak. Drie dingen voelen nog als standaard-egui:
- de kopregel
- de vensters Tape, Export en Settings
- de invoervelden op de console (tempo, maatsoort, ingang)

0.22.0 trekt die gelijk met de rest. De UI is sinds 0.21.1 Engels.

## Besluiten

| Vraag | Besluit |
|---|---|
| Kopregel | Rustig en leeg. Links File en Edit, in het midden de projectnaam, rechts de status, de opslagmelding en het icoontje voor volledig scherm |
| Uitvoerkeuze en Refresh | Weg uit de kopregel; ze staan al in Settings, dat de apparaten opnieuw ophaalt als het venster opengaat |
| Simple / 388 | Een schuifschakelaar op de console, bovenaan de MASTER-kolom |
| Vensters | Zoals de mockup `design/ref/Tape-instellingen en export@1x.png`: crème papier met een donkere titelbalk |
| Tempo, maatsoort, ingang | Kleine LED-displays in de stijl van de teller |

## Kopregel

- **Links**: File en Edit als menu's, in Plex 11 punten, zonder knopkader (alleen tekst, oplichtend bij hover).
- **Midden**: de projectnaam, met "(modified)" erachter als er wijzigingen zijn.
- **Rechts**: de statusmelding (ingekort, gedimd, rood bij een fout), de opslagmelding (groen) en het icoontje voor volledig scherm.
- Achtergrond `PANEL_DARK` met een dunne lijn eronder.
- De Simple/388-knoppen, de uitvoerkeuze en Refresh verdwijnen uit de kopregel.

## Simple / 388 op de console

- In de MASTER-kolom komt een nieuw vlak bovenaan, `ROUTING`. Het bevat de schuifschakelaar met de standen SIMPLE en 388, dezelfde schakelaar als INPUT.
- Het gedrag blijft zoals het nu is: naar Simple gaan vraagt om bevestiging als de 388-instellingen anders zijn (`set_routing`).
- De tooltip van de schakelaar legt het verschil uit, met dezelfde tekst als de huidige knoppen.
- Het vlak verlaagt de ruimte boven de STEREO-fader. `master_bands` moet in beide standen onder `fader_top` blijven; een test borgt dat.

## Vensters op papier

Een nieuwe module `crates/ui/src/skin/paper.rs`:

- **`paper_window(ctx, id, title, note, open, width, add)`**: een `egui::Window` zonder eigen titelbalk.
  - Het frame is crème (`PAPER`, ongeveer `#e6dfcc`) met een donkere rand.
  - Bovenin tekenen we zelf een donkere titelbalk (`PANEL_DARK`): de titel in Plex SemiBold 14, rechts een korte notitie (bijvoorbeeld "per project") en een sluitkruisje.
  - Verslepen gaat via de titelbalk.
  - Binnen het venster gelden lichte visuals: donkere tekst; selectie, vinkjes en schuifjes in roestbruin (`RUST`, ongeveer `#b0501e`); invoervakken crème met een donkere rand.
- **`section_title(ui, text)`**: kleine kapitalen in SemiBold met wat letterafstand, zoals "MACHINE" en "TAPE SPEED" in de mockup.
- **`choice_cards(ui, options, &mut value)`**: kaarten naast elkaar met een rondje en een label, optioneel met een tweede regel.
  - De gekozen kaart is zwart met oranje tekst, zoals 7½ IPS in de mockup.
  - De andere kaarten zijn crème met een donkere rand.
- **`switch_pair`**: twee vakken naast elkaar, gebruikt voor Clean / 388. Het gekozen vak is zwart.
- **Sliders**: met label links en waarde rechts in Plex Mono, de vulling in `RUST`.
- **Knoppen**: de hoofdknop (Export) is roestrood met witte tekst, de andere knoppen (Cancel) crème met een rand.

**Tape** (360 breed, notitie "per project"):
- MACHINE: één kaart "388 Studio 8" met "2 heads · 1/4 inch · 8 tracks".
- TAPE SPEED: drie kaarten, met eronder "7½ IPS is the original speed of the 388".
- TAPE TYPE: drie kaarten met hun korte omschrijving als tweede regel.
- CHARACTER: Clean / 388 als `switch_pair`, dan de vier schuifjes, dan de dbx-vinkjes op één regel.
- De werking blijft precies zoals nu.

**Export** (440 breed):
- WHAT: kaarten "Stereo bounce" en "Separate tracks". Beide kunnen tegelijk aan, zoals nu, dus de kaarten werken hier als aan/uit.
- TRACKS: de vinkjes in twee kolommen, met het nummer in Plex Mono ("01 Kick").
- PROCESSING: de drie keuzes als rondjes onder elkaar.
- RANGE: de twee vinkjes.
- Formaat, bitdiepte en samplerate als drie keuzelijsten naast elkaar, met hun label erboven.
- Een regel met de bestandsnaam ("File name: Project_01_Kick.wav") en de map.
- Rechtsonder Cancel en Export. Tijdens het exporteren een voortgangsbalk in `RUST`.

**Settings** (460 breed): de bestaande onderdelen (audio, metronoom, REPEAT) als secties met `section_title`, in dezelfde papierstijl.

**Dialogen** (niet opgeslagen, naar Simple): dezelfde stijl, kleiner.

## LED-displays op de console

`led_field` in `skin/keys.rs`: een donker vak (`#120807`) met een dunne rand, de tekst in Plex Mono in `LED`-rood.
- **TEMPO**: een `DragValue` in deze stijl (slepen of klikken en typen, zoals nu), bijvoorbeeld "120.0".
- **MAAT** (TIME SIG): een keuzelijst in deze stijl ("4/4").
- **Ingang** in de strip: een keuzelijst in deze stijl ("IN 1", "NO IN").

Een gescopede stijl regelt dat: binnen het display gelden een donkere achtergrond, rode monospace-tekst en geen lichte rand. De opengeklapte lijsten blijven gewone menu's.

## Kleinigheden

- Onder een lege strip staat geen "empty" meer, maar een gedimd streepje. Dubbelklikken om een naam te geven blijft werken.
- In de eenvoudige stand krijgen de lege delen een donker vlak (`PANEL_DARK` op 50%). Dat geldt voor de EFFECT RTN-kolom onder ECHO, en voor de rechterkolom onder het transport in beide standen. Zo oogt het bedoeld in plaats van leeg.

## Code

- `skin/paper.rs`: venster en bouwstenen.
- `lib.rs`: de kopregel, de drie vensters en de twee dialogen naar `paper_window`, en de tempo- en maatvelden naar `led_field`.
- `console.rs`: het ROUTING-vlak en het ingangsdisplay.

## Testen

- **Hoogtes**: de MASTER-kolom met het ROUTING-vlak past in beide standen boven de faders (uitbreiding van `the_faders_start_below_all_bands`).
- **Bestaande tests**: die van export, tape en routing blijven slagen; de werking verandert niet.
- **Uiterlijk**: dat test Ton in de app, vergeleken met de mockup.
