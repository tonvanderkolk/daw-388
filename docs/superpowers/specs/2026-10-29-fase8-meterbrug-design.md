# Ontwerp fase 8, stap 1: meterbrug en bandrecorder (0.19.0)

Datum: 29-10-2026
Status: goedgekeurd in gesprek

## Doel

De vintage skin volgens A2 (`design/A2 · Vintage, dichter bij de 388@2x.png`). Fase 8 wordt opgedeeld:

- **0.19.0**: meterbrug met VU-meters, bandrecorder met spoelen, lettertype, meeschalen met het venster.
- **0.20.0**: kanaalstrips en masters dichter bij A2 (fadercaps, schaal, ASSIGN, nummer en naam onderaan) en de la in vintage stijl.
- **0.21.0**: de vensters Tape, Export en Instellingen in de stijl van `design/ref/Tape-instellingen en export@1x.png`.

Dit document gaat over 0.19.0.

## Besluiten

| Vraag | Besluit |
|---|---|
| Opdeling | Drie releases, zie boven |
| Balkmeters | De VU-brug vervangt de balkmeters van de kanalen en de stereo-/send-meters |
| Naaldgedrag | Echte VU: gemiddeld, ±300 ms, lichte overshoot; 0 VU = −18 dBFS |
| Past niet in venster | De hele UI zoomt mee (70% tot 125%), daaronder scrollen |
| Lettertype | IBM Plex Sans Condensed en IBM Plex Mono (OFL), zoals in de mockup |

## Indeling

- Bovenin de houten omlijsting de **meterbrug** over de volle breedte:
  - **TRK 1-8**: acht kleine VU's met eronder "TRK" en het spoornummer.
  - Twee grote VU's: **STEREO L / AUX** en **STEREO R / EFFECT**.
  - Rechts de **naamplaat**: "DAW 388" (rood, cursief, vet), "STUDIO 8" en "tape-emulatie · 8 sporen". Geen Tascam-logo.
- Daaronder links de **mixer** zoals nu, zonder balkmeters. Rechts de **bandrecorder**: het rookglazen venster met spoelen en daaronder het bestaande transportblok (teller, toetsen, pitch, snelheid, BOUNCE en EXPORT).
- Het losse "DAW 388 / STUDIO 8"-label naast het transport vervalt.
- De kopregel en de la blijven zoals ze zijn; die krijgen in 0.20.0 hun vintage vorm.

## VU-meters

### Engine

- Naast de bestaande pieken houdt de engine per meterpunt een **energiesom** bij: de som van de kwadraten en het aantal samples sinds de laatste uitlezing. Dat geldt voor:
  - per kanaal PRE en POST (dezelfde punten als `TrackPeaks`)
  - de stereo-uitgang L en R
  - de AUX- en EFF-send na hun master
- Opslag lock-free in atomics, zoals de pieken. De audiothread telt op, de UI leest en zet op nul (`swap`).
- De uitlezing geeft de **gemiddelde energie** over de periode sinds de vorige uitlezing (0 als er geen samples waren).
- Nieuwe functies naast de bestaande: `take_track_energy`, `take_master_energy` en `take_send_energy`. Ze geven gemiddelden in hetzelfde formaat als de piekvarianten. De piekfuncties blijven, want OL leest de PRE-piek.

### Naald

- Een tweede-ordesysteem, licht ondergedempt, in `skin/vu.rs`. De invoer is het RMS-niveau in VU, met 0 VU = −18 dBFS RMS.
- Een sprong van stilte naar 0 VU bereikt na ongeveer 300 ms 99% van zijn eindwaarde, met hooguit 1,5% overshoot.
- De schaal loopt van −20 tot +3 VU. Daarbuiten blijft de naald tegen de aanslag. Stilte (geen energie) zakt naar −20.
- De naaldfysica staat los van egui: een `Needle` met `step(dt, level_vu)` en `position()`, en is apart te testen.
- Wat er getekend wordt: een crème wijzerplaat, de schaalverdeling met het rode deel vanaf 0 VU, de letters "VU", een zwarte naald vanaf een draaipunt onder de plaat, en een donkere rand.

### Bronnen

- **TRK 1-8**: het kanaal, PRE of POST volgens de bestaande schakelaar TRACK METERS (`meter_post`), zoals de balkmeters nu.
- **OL**: het bestaande OL-lampje (PRE-piek, `OL_HOLD`) komt als rode stip rechtsboven in de VU van dat spoor.
- **Grote meters**: de bestaande METER-schakelaar (`MeterSwitch`). Bij **L-R** tonen ze de stereo-uitgang, bij **AUX-EFF** de AUX- en EFF-send.
  - Onder de meters staan de labels "STEREO L / AUX" en "STEREO R / EFFECT". Het deel dat actief is, is helder en het andere gedimd.
  - Op de grote meters zit ook een OL-stip, op basis van de piek van de getoonde bron boven −3 dBFS (dezelfde `overload`-regel als de kanalen).
- **Wat vervalt**: de balkmeters in de kanaalstrips en in de master-sectie. De schakelaars TRACK METERS en METER blijven waar ze staan.
- **Wat blijft**: de smalle TRK-balkjes in de rijen van de MONITOR-sectie (388-stand). Dat is het bussignaal per spoor, geen kanaalmeter, en het hoort bij de routering.

## Bandrecorder

- Een donker rookglazen venster (`skin/reels.rs`) met twee spoelen en een stukje band ertussen. Bovenin links de tape-regel.
  - Die regel komt uit een nieuwe `tape_line` in de stijl van de mockup: "456-stijl · 7½ IPS · dbx aan", of "Clean" als het project op Clean staat.
  - "dbx aan" verschijnt als minstens één groep dbx heeft, met "dbx 1-4" of "dbx 5-8" als dat maar voor één groep geldt, en "dbx uit" zonder dbx.
- **Draaien**: de hoeksnelheid volgt uit de verandering van de bandpositie (`position_seconds`) per frame. PLAY, varispeed, REW, F FWD, CUE-zoeken en REPEAT kloppen dus vanzelf.
  - Een sprong groter dan 2 seconden tussen twee frames (RTZ, klikken in de la, REPEAT naar het CUE-punt) telt niet als draaien.
  - De weergave: band met 7½ ips over een naaf met een vaste visuele schaal. Draaien wordt begrensd op 3 omwentelingen per seconde, zodat F FWD niet stroboscopisch wordt.
- **Wikkel**: de linkerspoel heeft een wikkelstraal van vol naar leeg en de rechterspoel van leeg naar vol, over `positie / lengte`.
  - De lengte is het einde van de laatste regio, minimaal 3 minuten.
  - Beide spoelen houden altijd een kleine kern zichtbaar.
- **Spoel**: drie spaken met een naaf, in de bruine tinten van de mockup. Alleen tekenen, niet klikbaar.

## Lettertype

- `IBMPlexSansCondensed-Regular`, `IBMPlexSansCondensed-SemiBold` en `IBMPlexMono-Medium` worden met `include_bytes!` ingebakken, vanuit `crates/ui/assets/fonts/`. Daar staan ook `OFL.txt` en een vermelding in de README.
- egui heeft geen gewichtsvariant per tekst. Daarom:
  - De Proportional-familie wordt Plex Sans Condensed Regular.
  - Een eigen familie `"plex-bold"` (SemiBold) is er voor de labels die nu `.strong()` gebruiken. Een helper `bold(text)` zet die familie, in `skin/fonts.rs`.
  - De Monospace-familie wordt Plex Mono Medium. Die gebruikt de LED-teller al via `.monospace()`.
- Het standaardfont van egui blijft als terugvaloptie in beide families, voor tekens die Plex niet heeft.
- De omzetting van bestaande `.strong()`-aanroepen naar `bold` gebeurt in 0.19.0 alleen voor de brug en de bandrecorder. De rest volgt in 0.20.0.

## Meeschalen

- Ontwerpmaat: 1800 × 1360 logische punten. Dat is de console met de brug plus de dichte la.
- `zoom = clamp(min(breedte / 1800, hoogte / 1360), 0,70, 1,25)`, afgerond op 0,05 om knipperen bij het slepen van het venster te voorkomen.
- Breedte en hoogte zijn de binnenmaat van het venster in fysieke punten, gedeeld door de native `pixels_per_point` van het scherm.
- Toegepast via `ctx.set_zoom_factor` in het begin van `ui()`. Dat gebeurt alleen als de waarde verandert.
- Onder 70% scrollt de console, zoals nu.
- De berekening is een losse functie, `zoom_for(inner_size)`, en apart te testen.

## Code

- Nieuwe map `crates/ui/src/skin/` met:
  - `mod.rs`: `palette` verhuist hierheen, de rest van `lib.rs` gebruikt `skin::palette`
  - `fonts.rs`
  - `vu.rs`: `Needle` en het tekenen van een VU
  - `reels.rs`
  - `bridge.rs`: de indeling van de brug en de naamplaat
- `lib.rs` krijgt per meter een `Needle` in plaats van de `*_meter_db`-velden waar die alleen voor balken dienden. Het piekdeel van `update_meters` blijft voor OL.
- `level_bar` blijft alleen voor de MONITOR-rijen.

## Testen

- **Engine**: een constante sinus met amplitude A geeft een gemiddelde energie van A²/2 (binnen 1%). Na een uitlezing staat de som op nul. Zonder samples is het resultaat 0.
- **Naald**:
  - Een constante invoer op 0 VU staat na 300 ms binnen 1% van 0 VU.
  - De overshoot is hooguit 1,5%.
  - Stilte zakt terug naar −20 en blijft daar.
  - Waarden boven +3 blijven op +3.
- **Spoelen**: de wikkelstraal bij positie 0 en bij het einde. Een sprong van meer dan 2 s draait niet. De begrenzing op 3 omwentelingen per seconde.
- **Zoom**: 1680 × 950 geeft 0,70, 1800 × 1360 geeft 1,0, 2560 × 1600 geeft 1,20 en 2880 × 1800 geeft 1,25. De afronding is op 0,05.
- **`tape_line`**: Clean, en dbx op geen, één of twee groepen.
- Het uiterlijk test Ton in de app.

## Buiten deze stap

- Kanaalstrips, masters, de la en kopregel in vintage vorm: 0.20.0.
- Vensters Tape, Export en Instellingen: 0.21.0.
- Klikbare spoelen of bandanimatie bij opnemen.
