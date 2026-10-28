# Ontwerp fase 5: bewerken (0.17.0 en 0.18.0)

Datum: 28-10-2026
Status: goedgekeurd in gesprek

## Doel

Niet-destructief bewerken van de 8 sporen: regio's verschuiven, inkorten, splitsen, knippen, kopiëren en plakken per spoor, en een tijdvak over alle sporen tegelijk knippen of invoegen zoals een splice op band. Undo en redo tot 10 stappen. Het bewerken gebeurt in een la met de sporen en een tijdlijn, die uit de armsteun onder de mixer schuift (ontwerp A2).

Fase 5 wordt opgedeeld:

- **0.17.0, regio's onder de motorkap:** het regiomodel, het uitrekenen van een spoor, projectformaat 3, opnames en import als regio, gezamenlijke undo en redo. Nog zonder la.
- **0.18.0, de la:** het bewerkingspaneel met alle bediening.

Pingpong naar een vrij spoor kan al via de bussen sinds 0.8.0 en valt buiten deze fase.

## Besluiten

| Vraag | Besluit |
|---|---|
| Soort bewerken | Regio's per spoor en een tijdvak over alle 8 sporen (splice) |
| Overlap | Regio's overlappen nooit. Wat op een plek valt vervangt wat er lag; de bron blijft, dus een rand is terug te slepen |
| Raster | Knop RASTER (aan of uit): tellen van de metronoom. Altijd ook randen van regio's, nulpunt, CUE-punt en afspeelpositie. Alt ingedrukt: tijdelijk vrij |
| Randen | Vast en niet instelbaar: een fade van 3 ms aan een vrije rand, een crossfade van 3 ms waar twee regio's tegen elkaar liggen, niets waar een regio zijn bron naadloos voortzet (na SPLITS) |
| Undo | Eén lijst van 10 stappen voor alles wat de inhoud van de sporen verandert (bewerken, opnemen, importeren, Wis), met redo. Mixer, transport en instellingen niet |
| Plek van het paneel | Een la die uit de leren armsteun onder de mixer schuift, over de faders |
| Splice | Knippen van een tijdvak over alle sporen schuift alles erna naar voren; plakken voegt in en schuift op. Op één spoor laat knippen een gat |
| Selecteren | Alleen hele regio's op een spoor; een tijdvak over alle sporen in de tijdlijnbalk |
| Importeren | Als regio op de afspeelpositie (vervangt alleen dat stuk), en bestanden uit de Finder op een spoor slepen |
| Afspelen van regio's | De UI rekent elk spoor uit tot één buffer en stuurt die met `SetTrack`; engine, tape-model en export blijven ongewijzigd |

## 0.17.0: regio's

### Model

Nieuwe module `crates/project/src/regions.rs`, zonder UI en zonder audio-I/O.

- `Region { source: SourceId, start: f64, offset: f64, length: f64, name: String }`. Tijden in seconden: `start` is de plek op de band, `offset` waar de regio in zijn bron begint, `length` hoe lang hij is. `offset + length` ligt nooit voorbij de lengte van de bron.
- `SourceId` wijst naar een bron in een tabel van het project: het bestand (pad) en de duur in seconden. In het geheugen hoort bij elke bron een gedeelde `Arc<MonoClip>`.
- Een spoor is `Vec<Region>`, gesorteerd op `start`, zonder overlap. Regio's korter dan 1 ms bestaan niet (een bewerking die ze zou maken laat ze weg).
- Bewerkingen zijn pure functies die een nieuwe lijst teruggeven, zodat undo de oude kan bewaren:
  - `place(track, region)`: zet een regio neer en snijdt weg wat eronder lag (een regio die er half onder ligt wordt korter, een regio die erdoorheen loopt wordt in tweeën gesplitst, een regio die er helemaal onder ligt verdwijnt).
  - `remove(track, index)` en `split(track, at)`.
  - `move_regions(tracks, selection, track_delta, time_delta)`: haalt eerst alle geselecteerde regio's weg en zet ze daarna met `place` op hun nieuwe plek, zodat ze elkaar niet wegsnijden. Een verschuiving die een regio voor spoor 1, na spoor 8 of voor 0:00 zou zetten, wordt begrensd: de hele groep stopt bij de rand.
  - `trim_start` en `trim_end`: een rand verschuiven, begrensd door de bron (niet voor het begin, niet voorbij het einde) en door 0:00 op de band. Wat daarbij onder de regio valt, wordt weggesneden zoals bij `place`.
  - `cut_range(tracks, from, to)` met aanschuiven: op elk spoor gaat het stuk eruit (regio's worden gesplitst op de randen) en alles erna schuift `to - from` naar voren. Geeft ook het uitgeknipte stuk terug als `Clipboard`.
  - `copy_range(tracks, from, to)`: hetzelfde stuk als `Clipboard`, zonder iets te veranderen.
  - `insert_range(tracks, at, clipboard)`: op elk spoor schuift alles vanaf `at` (een regio over `at` wordt eerst gesplitst) de lengte van het klembord op, daarna komen de regio's van het klembord erin.
  - `clear_range(tracks, from, to)`: het stuk eruit op alle sporen, zonder aanschuiven (WIS met een tijdvak).
  - `clear_track(track)`: Wis.
- `Clipboard` bewaart regio's met hun spoor en hun plek ten opzichte van het begin, plus de lengte (bij een tijdvak). Er zijn twee soorten: regio's (plakken vervangt, op hun eigen spoor) en een tijdvak (plakken voegt in over alle sporen).
- Het einde van een spoor is het einde van zijn laatste regio; het einde van het nummer is het verste spoor.
- **Onbekende lengte:** naast zijn regio's kan een spoor één `Unresolved { source, name }` hebben: een bestand dat vanaf 0:00 het hele spoor vult, maar waarvan de lengte nog niet bekend is (een oud project dat opent, of "8 audiobestanden laden"). Zodra de bron geladen is, wordt het een gewone regio. Zolang een spoor zo'n verwijzing heeft, doen de bewerkingen niets op dat spoor behalve Wis, zijn de bewerkingen over alle sporen (tijdvak, splice) uitgeschakeld en telt het spoor niet mee voor het einde van het nummer. Ontbreekt de bron, dan blijft het zo tot Wis; de verwijzing wordt bewaard.
  - Op zo'n spoor kan niets nieuws komen: REC FUNCTION, importeren en bestanden slepen worden geweigerd met een melding ("Spoor 3 wordt nog geladen" of "Spoor 3: audio ontbreekt, wis het spoor om erop op te nemen").
  - Undo-stappen bewaren de verwijzing mee. Is een bron geladen, dan wordt elke verwijzing naar die bron een regio, ook in de stappen van undo en redo (de lengte is een feit over het bestand).

### Uitrekenen

- `render_track(regions, sources, rate) -> Vec<f32>`: één buffer vanaf 0:00 tot het einde van de laatste regio, op de samplerate van de engine. Stilte tussen regio's.
- Beide randen van een regio worden los afgerond naar hele samples op `rate` (`round(start * rate)` en `round((start + length) * rate)`), zodat regio's die tegen elkaar liggen ook op een andere samplerate aansluiten zonder gat of overlap. De bronnen zijn al op `rate`.
- Randen (lineair, 3 ms; bij een regio korter dan 6 ms de helft van zijn lengte):
  - **Vrije rand** (stilte ernaast): fade-in of fade-out.
  - **Twee regio's tegen elkaar:** een crossfade van 3 ms, gecentreerd op de naad. Beide regio's lopen daarvoor 1,5 ms door in hun bron voorbij hun rand; heeft een bron daar geen audio meer, dan fadet dat deel naar nul. Zo ontstaat geen tik en geen gat.
  - **Naadloos voortgezet:** als de volgende regio dezelfde bron precies voortzet (zijn `offset` is het einde van de vorige, zoals na SPLITS), dan niets: het klinkt bit voor bit als voor de splitsing.
- Na een wijziging rekent een achtergrondtaak (`Platform::spawn`) de geraakte sporen uit. De uitkomst gaat met het bestaande volgnummer (`generation`) en `Command::SetTrack` naar de engine; een te late uitkomst voor een oudere stand wordt genegeerd, zoals bij het laden nu.
- Export: de exporttaak krijgt in plaats van een clip per spoor de regio's, de naam van het spoor en de gebruikte bronnen (`Arc<MonoClip>`). Op de exportthread worden de bronnen met `rubato` naar de exportsamplerate gezet en de sporen met `render_track` uitgerekend, zodat de export klinkt zoals de app. Het einde van het nummer, of een spoor audio heeft (stems) en de naam van een stembestand komen uit de regio's en de naam van het spoor.
- Bij een apparaatwissel worden de sporen opnieuw uitgerekend op de nieuwe samplerate.

### Spoor in de UI

- `TrackState.clip` wordt `regions: Vec<Region>` plus een naam. De sporen delen een tabel met bronnen: per bron het pad, de duur, de `Arc<MonoClip>` op zijn eigen samplerate en een kopie op de samplerate van de engine (zodat een bron maar één keer wordt omgezet), en de toestand `loading` of `missing`. `loading` en `missing` verhuizen dus van het spoor naar de bron; een spoor toont "laden" of "ontbreekt" als een van zijn bronnen dat is. Het geheugen per bron is zijn eigen samplerate plus de enginesamplerate; bij een apparaatwissel wordt de oude kopie vervangen.
- Een bron wordt één keer gedecodeerd, op de achtergrond, ook als meerdere regio's of sporen hem gebruiken. Een bron heeft een eigen volgnummer voor het laden; een spoor heeft een eigen volgnummer voor het uitrekenen (`generation`), zodat een te laat resultaat van een van beide wordt genegeerd.
- Een bron die ontbreekt, heeft als duur de grootste `offset + length` van zijn regio's; verder inkorten of terugtrekken dan dat kan niet.
- De naam van een spoor (voor de strook, de exportbestanden en de namen van opnames) is een eigen veld. Een leeg spoor krijgt de naam van de eerste audio die erop komt; Wis maakt de naam weer leeg.
- De knop om een bestand te laden in de strook zet het bestand als regio op dat spoor, op de afspeelpositie van het moment van kiezen. De regio komt erop zodra de bron gedecodeerd is; dat is ook het moment van de undo-stap ("Importeren").
- "8 audiobestanden laden..." en bestanden op de opdrachtregel vervangen de sporen zoals nu: elk spoor (1 tot 8 op naamvolgorde) wordt meteen leeg gemaakt en krijgt zijn bestand als verwijzing met onbekende lengte, die een regio op 0:00 wordt zodra de bron geladen is. De undo-stap ("Laden") wordt bij het kiezen gemaakt.
- Verandert de inhoud van een spoor buiten de muis om (een take, een import of een bron die geladen is), dan vervalt de selectie, zoals bij undo.
- Wis haalt alle regio's van het spoor weg.

### Projectformaat 3

- Per spoor `name` en `regions: [{ "file": "audio/Kick.wav", "start": 0.0, "offset": 0.0, "length": 12.5, "name": "Kick" }]` in plaats van `file`. Hetzelfde bestand mag in meerdere regio's voorkomen; het wordt één keer geladen.
- `length` mag alleen ontbreken bij de enige regio van een spoor, met `start` en `offset` 0: dat is op schijf de verwijzing met onbekende lengte (in het geheugen `Unresolved`), en de lengte wordt ingevuld zodra de bron geladen is. Een regio zonder `length` in een ander geval wordt bij het openen weggelaten met een waarschuwing in het log (de app schrijft zoiets nooit). Een project met formaatversie 2 of ouder opent zo: per spoor met audio één regio op 0:00 zonder `length`, met de naam van het bestand. Bij de eerstvolgende opslag wordt het formaat 3; een regio waarvan de bron ontbreekt en de lengte nog onbekend is, wordt bewaard zonder `length`, zodat de verwijzing nooit verloren gaat. Zo'n regio is stil en in de la een smalle markering met "ontbreekt".
- Een regio die verwijst naar een bestand dat ontbreekt, blijft in het project staan en is stil; de melding is dezelfde als nu bij ontbrekende audio. Een regio die langer is dan zijn bron (het bestand is vervangen), wordt na het laden ingekort tot de bron; ligt zijn `offset` al voorbij het einde, dan vervalt hij (de regel van 1 ms).
- Opslaan kopieert bronnen van buiten het project naar `audio/`, zoals nu (met achtervoegsel bij een naamconflict). Een bron die in meerdere regio's of op meerdere sporen staat, wordt één keer gekopieerd; het pad in de brontabel gaat daarna naar de kopie.
- Een oud project klinkt bijna hetzelfde als in 0.16.0; alleen begin en einde van elk spoor krijgen nu de fade van 3 ms. Dat komt in de changelog. Een project met formaatversie hoger dan 3 wordt geweigerd.

### Opnemen

- Een take wordt na de opname als 24-bit WAV `NN_Naam_opnameN.wav` in `audio/` gezet, alleen het opgenomen stuk (geen samenvoeging meer met de oude audio). Daarna wordt hij een bron en een regio op zijn startpositie, en `place` vervangt wat eronder ligt. De startpositie van de schrijver is in samples op de samplerate van de take (latency-gecorrigeerd) en wordt bij het plaatsen omgerekend naar seconden.
- `take::lay_over` en het herschrijven van het hele spoor vervallen. Takes van hetzelfde spoor blijven op volgorde verwerkt worden.
- Een opname begint nooit voor 0:00: een take die door latencycorrectie voor het begin zou vallen, verliest dat stuk (via `offset`).
- Een take die klaar is na STOP wordt altijd geplaatst, ook als het spoor intussen gewist of iets ongedaan is gemaakt: hij wordt dan een eigen undo-stap bovenop. Een take van een project dat intussen gesloten is of vervangen door een ander of nieuw project, wordt niet geplaatst (het bestand blijft in de `audio/` van het oude project). Daarvoor krijgt het project een volgnummer dat bij openen en nieuw verandert.

### Undo en redo

- Eén stapel van hooguit 10 stappen. Een stap heeft een naam ("Opname spoor 3", "Splice", "Verschuiven", "Importeren", "Laden", "Wis") en de regiolijsten, verwijzingen met onbekende lengte en namen van de sporen die hij raakte, van voor de stap. Bronnen zijn gedeeld, dus een stap kost weinig geheugen.
- Cmd+Z zet terug, Cmd+Shift+Z doet opnieuw. Bij terugzetten bewaart de stap eerst de huidige stand van die sporen, zodat opnieuw die stand terugzet (en omgekeerd). Een nieuwe stap gooit de redo-lijst weg. In het menu Bewerken: "<naam> ongedaan maken" en "<naam> opnieuw".
- Terugzetten en opnieuw maken het project gewijzigd (autosave), halen de selectie weg en laten het klembord staan.
- Bestanden worden nooit verwijderd; een ongedane opname blijft in `audio/` staan.
- Een nieuw of geopend project begint met een lege stapel.
- Tijdens een opname zijn undo en redo uitgeschakeld.

## 0.18.0: de la

### Indeling

- De leren armsteun onder de mixer is de greep. Klikken schuift de la uit of in. Uitgeschoven ligt de la over het onderste deel van de mixer; meterbrug, knoppen en transport blijven zichtbaar. De bovenrand van de la sleep je voor de hoogte; die wordt per project bewaard in een optioneel blok `editor` in `project.json` (`open`, `height` in pixels, `zoom` in pixels per seconde en `snap`). Het blok hoort bij formaat 3 zonder nieuwe versie; ontbreekt het, dan is de la dicht, 320 pixels hoog, toont hij het hele nummer en staat RASTER aan.
- Werkbalk: SPOREN, SPLITS (S), KNIP (Cmd+X), KOPIEER (Cmd+C), PLAK (Cmd+V), WIS (Delete of Backspace), RASTER, zoom (−, PAST, +), en rechts de stand van undo (bijvoorbeeld "Undo 3/10") met tempo en maatsoort.
- Links per spoor het nummer en de naam (dubbelklik om de naam te wijzigen, een undo-stap). Daarboven een tijdlijnbalk met maten en tellen als RASTER aan staat, anders min:sec, en het nulpunt en CUE-punt als merktekens.
- Regio's tonen hun naam en golfvorm. De golfvorm komt uit een overzicht per bron (minimum en maximum per blok van 256 samples), één keer op de achtergrond berekend bij het laden of na een opname.
- De afspeelpositie is een rode lijn over alle sporen. Tijdens het afspelen bladert de la mee zodra de positie uit beeld loopt.
- De sneltoetsen van de la werken alleen als de la open is en geen tekstveld de focus heeft (naam van een spoor, tempo).

### Bediening

- **Tijdlijnbalk:** klikken zet de band daar (`Locate`). Slepen selecteert een tijdvak over alle 8 sporen (gekleurd over alle sporen).
- **Regio:** klikken selecteert, Shift voegt toe of haalt weg. Klikken op een leeg stuk spoor haalt de selectie weg. Slepen verschuift de geselecteerde regio's samen, ook naar een ander spoor (verticaal per hele sporen); bij loslaten worden ze met `place` neergezet (één undo-stap). Tijdens het slepen ziet de la de nieuwe plek meteen; het geluid volgt na het uitrekenen.
- **Rand:** slepen aan de linker- of rechterrand (een strook van 6 pixels) kort de regio in of trekt hem terug, begrensd door de bron.
- **SPLITS:** de geselecteerde regio's op de afspeelpositie; zonder selectie alle regio's onder de afspeelpositie.
- **KNIP, KOPIEER, PLAK:** met regio's geselecteerd gaan die naar het klembord; KNIP haalt ze weg en laat een gat. PLAK zet ze op de afspeelpositie, elk op zijn eigen spoor, met dezelfde onderlinge afstand, en vervangt wat eronder ligt. Met een tijdvak geselecteerd is KNIP een splice (eruit en aanschuiven) en KOPIEER kopieert het tijdvak; een geplakt tijdvak wordt op de afspeelpositie ingevoegd over alle sporen en schuift de rest op. De selectie heeft voorrang: zijn er regio's én een tijdvak, dan werkt de knop op het tijdvak.
- **WIS:** de geselecteerde regio's weg (een gat), of met een tijdvak het stuk eruit zonder aanschuiven.
- **RASTER:** bij slepen van regio's, randen en tijdvakken klikt de plek vast op de dichtstbijzijnde tel (als RASTER aan staat), rand van een andere regio, het nulpunt, het CUE-punt of de afspeelpositie, binnen 8 pixels. Alt ingedrukt: vrij. Het raster volgt tempo en maatsoort van de metronoom; maat 1 begint aan het begin van de band, zoals de klik.
- **Zoom:** Cmd plus scrollen zoomt rond de muis, scrollen schuift door de tijd, PAST toont het hele nummer.
- **Bestanden slepen:** een audiobestand uit de Finder op een spoor in de la wordt daar een regio, op de plek waar je loslaat (met raster). Meerdere bestanden komen op opeenvolgende sporen vanaf dat spoor (wat na spoor 8 valt, vervalt met een melding); samen één undo-stap. Hiervoor moet slepen en neerzetten in het venster aan staan (`crates/app-desktop/src/main.rs`).
- Tijdens een opname is bewerken uitgeschakeld (de la blijft zichtbaar).

## Testen

- Model: een spoor met een verwijzing van onbekende lengte (bewerkingen slaan het over, Wis werkt, het telt niet mee voor het einde); elke bewerking, met randgevallen: `move_regions` met regio's op meerdere sporen en tegen de rand van spoor 1 of 8; `clear_range`; `place` over het begin, het einde en het midden van een regio en over meerdere regio's; inkorten en terugtrekken tot de grens van de bron en tot 0:00; splitsen op een rand (doet niets); `cut_range` midden in een regio en over een leeg spoor; `insert_range` midden in een regio; regio's korter dan 1 ms verdwijnen.
- Uitrekenen: stilte tussen regio's, fades aan vrije randen, crossfade tussen twee regio's (geen gat), een gesplitste regio klinkt bit voor bit als het origineel, randen sluiten aan op een andere samplerate, lengte gelijk aan het einde van de laatste regio.
- Projectformaat: omzetting van formaat 2 naar regio's (ook met een ontbrekend bestand: de verwijzing blijft na opslaan), opslaan en opnieuw openen van regio's, dezelfde bron in twee regio's, een te lange regio, een `offset` voorbij de bron.
- Opnemen: een take wordt een regio op de goede plek en vervangt alleen dat stuk; een take voor 0:00 verliest het stuk ervoor; een take na Wis wordt een eigen stap; een take na het openen van een ander project wordt niet geplaatst; een take op een andere samplerate dan de engine (na een apparaatwissel) komt op de goede plek.
- Undo en redo: hooguit 10 stappen, redo zet de stand van na de stap terug, redo verdwijnt na een nieuwe stap, bestanden blijven staan.
- Export: het einde van het nummer en de stems volgen de regio's.
- De la zelf test Ton. Per versie slagen opmaakcontrole, Clippy en alle tests.

## Documentatie

Met 0.17.0 worden in `PROJECT.md` de delen Projectmap (formaat 3, regio's), Opnemen (een take wordt een regio), Laden (bronnen) en Export bijgewerkt; met 0.18.0 komt er een deel Bewerken bij.

## Buiten deze fase

- Instelbare fades en crossfades (de vaste blijven), regio's in lagen, een gereedschap om binnen een regio een stuk te selecteren.
- Undo van mixerinstellingen.
- De vintage vormgeving van de la (fase 8).
