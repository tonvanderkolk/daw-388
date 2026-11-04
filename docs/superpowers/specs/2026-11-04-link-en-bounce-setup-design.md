# Ontwerp: LINK per paar (0.34.0) en Bounce Setup (0.35.0)

Datum: 04-11-2026
Status: goedgekeurd in het gesprek, ter controle

## Doel

Twee wensen uit het gebruik:

1. Een stereobestand komt als twee mono-sporen binnen (links op het spoor, rechts op het volgende). Daarna moet je alles dubbel doen: twee faders, twee keer knippen. Een koppeling per paar lost dat op, maar je moet er ook vanaf kunnen blijven.
2. Live mixen van een paar sporen naar andere sporen (bijvoorbeeld 1 tot en met 6 naar 7 en 8) kan alleen in de 388-stand, en daar moet je alle routing met de hand zetten. Een Bounce Setup zet het in één keer.

Beide blijven trouw aan de 388: ze zetten alleen knoppen die er al zijn. LINK is een kleine toevoeging, zoals MUTE, SOLO en DIRECT dat al zijn.

## Versies

| Versie | Inhoud |
|---|---|
| 0.34.0 | LINK per paar, stereo-import koppelt, twee uitlijningen in de MASTER-kolommen |
| 0.35.0 | Bounce Setup |

## Deel 1: LINK per paar (0.34.0)

### Paren

Vaste paren: 1-2, 3-4, 5-6 en 7-8, dezelfde paren als ASSIGN. Een spoor hoort dus bij precies één paar; het oneven spoor is de linkerkant, het even spoor de rechterkant.

### Waar je hem schakelt

- **Console**: een kleine toets LINK met lampje in het bovenste vlak (`BAND_TRACK`) van de oneven strip, naast LOAD en CLEAR. Brandt hij, dan verbindt een dun lijntje de spoornummers onder de faders van het paar.
- **TRACKS**: een schakelicoontje tussen de koppen van de twee sporen. Klikken zet de koppeling aan of uit.
- Beide doen hetzelfde. De tooltip: "LINK: tracks N and N+1 as a stereo pair".

### Wat er gekoppeld is

Verandert iets aan een spoor van een gekoppeld paar, dan krijgt het andere spoor dezelfde waarde, met deze uitzonderingen:

| Instelling | Gedrag |
|---|---|
| Fader, MUTE, SOLO, REC FUNCTION | gelijk |
| Kanaal: bron, TRIM, EQ, AUX (ook PRE), EFF, ASSIGN, L-R, DIRECT, AUTO | gelijk |
| PAN van het kanaal | gespiegeld: het even spoor krijgt `-pan`. Zo is PAN de breedte |
| MONITOR: volume en mute | gelijk |
| MONITOR: pan | gespiegeld |
| Ingang (MIC en LINE) | doorlopend: IN n op het oneven spoor geeft IN n+1 op het even spoor. Zonder ingang n+1 krijgt het even spoor geen ingang (NO IN) |
| Naam | los; elk spoor houdt zijn eigen naam |

De koppeling werkt in beide richtingen: aan het even spoor draaien verandert het oneven spoor op dezelfde manier.

Technisch: na het tekenen van de console en de TRACKS-koppen vergelijkt de app elk gekoppeld paar met de stand van het vorige frame. Het spoor dat veranderd is, geeft zijn waarden door. Zo hoeft geen enkele knop iets van de koppeling te weten. Eén functie `link_from(source, target)` doet het kopiëren met de uitzonderingen hierboven, en is los te testen.

### Bewerken in TRACKS

- **Kiezen**: kies je een regio op een gekoppeld spoor, dan wordt ook de regio op het andere spoor gekozen die op precies dezelfde plek begint en even lang is (de tegenhanger). Is die er niet, dan alleen de gekozen regio.
- **Verschuiven, knippen, kopiëren, plakken, splitsen, fades, wissen**: werken via de selectie en gaan dus vanzelf op beide sporen. Verschuiven naar een ander spoor verschuift het paar naar het andere paar (alleen als het doelpaar ook gekoppeld is; anders blijft verschuiven naar een ander spoor uit voor deze selectie).
- **LOAD** op een gekoppeld paar laadt een stereobestand over beide sporen; een monobestand komt op beide sporen.
- **CLEAR** op een gekoppeld spoor wist beide sporen, in één undo-stap.
- Elke bewerking blijft één undo-stap.

### Aan en uit

- **Aanzetten**: het even spoor neemt de instellingen van het oneven spoor over volgens de tabel. Staat de PAN van het oneven spoor in het midden, dan gaat het paar naar links en rechts (−1 en +1), want twee keer midden is geen stereo.
- **Uitzetten**: alles blijft staan zoals het is; daarna zijn de sporen weer los.
- Zoals alle mixerstanden zit LINK niet in undo.

### Stereo-import

- Een stereobestand op een oneven spoor koppelt het paar meteen.
- Op een even spoor (bijvoorbeeld 2 en 3) blijven de sporen los, zoals nu.
- In Settings komt de keuze **Link stereo imports**, standaard aan. Staat hij uit, dan werkt de import zoals nu.
- De keuze is een app-voorkeur in `settings.json` (`link_stereo_imports`, ontbreekt: aan).

### Opslaan

- In `project.json` krijgt `mixer` het veld `links`: vier booleans voor 1-2, 3-4, 5-6 en 7-8. Ontbreekt het, dan is niets gekoppeld; oudere projecten openen dus zoals ze waren.
- De formaatversie blijft 3: het veld is optioneel.

### Eenvoudige stand

LINK werkt in beide standen. In de eenvoudige stand zijn de meeste kanaalinstellingen vast, dus daar gaat het vooral om fader, PAN, MUTE, SOLO, REC en de ingang.

## Deel 2: twee uitlijningen (0.34.0)

1. **Namen onder de MASTER-faders**: PGM MASTERS en STEREO MASTER staan nu op verschillende hoogtes. Ze komen allebei op de regel van de spoornamen onder de kanaalfaders. ODD en EVEN blijven onder hun dB-waarde.
2. **MONO**: de toets zit te dicht op de lijn onder het vlak. Er komt minstens 10 punten ruimte tussen de toets en de lijn, in de gewone en de compacte console. De bestaande tests op de hoogtes van de vlakken blijven slagen.

## Deel 3: Bounce Setup (0.35.0)

### Waar

- **Edit, Bounce Setup...**
- Een toets **BOUNCE** in het ROUTING-vlak van de MASTER-kolom, naast de schakelaar SIMPLE/388.
- In de eenvoudige stand vraagt hij eerst om naar 388 te gaan, met de bestaande bevestiging als de 388-instellingen anders zijn. Daarna opent het venster.

### Het venster

Een papiervenster (`paper_window`), ongeveer 400 breed:

- **SOURCES**: acht aan/uit-toetsen (1 tot en met 8, met de spoornaam eronder). Gekoppelde paren gaan samen aan en uit.
- **TARGET**: kaarten voor één spoor (1 tot en met 8) of een paar (1-2, 3-4, 5-6, 7-8).
- **Twee vinkjes**, allebei standaard aan:
  - **Silence the sources in MONITOR**: je hoort de mix via het doel, niet dubbel.
  - **Link the target pair** (alleen bij een paar).
- Een spoor kan niet tegelijk bron en doel zijn: kies je een doel, dan gaan die sporen uit bij SOURCES en zijn ze daar niet te kiezen.
- Staat er al iets op het doel, dan staat er een waarschuwing: "Track 7 has audio; recording replaces it where you record."
- **Apply** (de hoofdknop) en **Cancel**. Apply kan pas met minstens één bron en een doel.

### Wat Apply zet

Het doel bepaalt de bus. Bus n gaat naar spoor n; ASSIGN 1-2 voedt bus 1 en 2, en PAN verdeelt tussen de oneven bus (links) en de even bus (rechts).

**Bronnen**:
- bron RMX (de band);
- ASSIGN alleen op het paar van het doel, de andere ASSIGN-toetsen uit;
- L-R en DIRECT uit;
- bij een mono doel: PAN helemaal naar de kant van de bus (oneven doel: links, even doel: rechts); bij een paar blijft PAN zoals hij is;
- met het eerste vinkje: in de MONITOR op mute.

**Doel**:
- REC FUNCTION aan (gewapend);
- DIRECT uit, ASSIGN uit, L-R uit;
- in de MONITOR aan: bij een paar links en rechts, bij mono in het midden;
- bij een paar met het tweede vinkje: LINK aan.

**Andere sporen** blijven zoals ze zijn. De PGM MASTERS (ODD en EVEN) blijven staan; staat er een helemaal dicht, dan meldt het venster dat.

Daarna sluit het venster en meldt de statusregel: "Bounce ready: press REC and PLAY". De app start de opname niet zelf; je mixt live met de faders terwijl de band loopt.

### Undo

Cancel verandert niets. De knopstanden die Apply zet, zitten niet in undo, zoals alle mixerstanden. De opname zelf is wel ongedaan te maken, zoals altijd.

### Code

Een functie `bounce_setup(sources, target, options)` die alleen de mixerstanden in `DawApp` zet en los te testen is; het venster roept haar aan.

## Testen

**0.34.0**
- `link_from`: fader en kanaal gelijk, PAN gespiegeld, ingang doorlopend, naam los.
- Een gekoppeld paar volgt een verandering aan het oneven en aan het even spoor.
- Aanzetten met PAN in het midden geeft −1 en +1.
- Een stereo-import op een oneven spoor koppelt het paar; met de voorkeur uit of op een even spoor niet.
- Een regio kiezen op een gekoppeld spoor kiest de tegenhanger; knippen en verschuiven werken op beide sporen in één undo-stap.
- `links` wordt bewaard en geopend; een project zonder `links` opent ongekoppeld.
- De hoogtes van de MASTER-vlakken passen nog boven de faders, gewoon en compact.

**0.35.0**
- Bounce Setup van 1 tot en met 6 naar 7-8 geeft precies de routing van het recept "Pingpong naar stereo" in PROJECT.md.
- Naar één spoor (bijvoorbeeld 1 tot en met 3 naar 4) zet PAN naar de goede kant.
- Een spoor kan niet bron en doel tegelijk zijn.
- Een opname na Bounce Setup geeft op 7 en 8 dezelfde audio als de bestaande bounce-test met de handmatige routing.

**Uiterlijk en gebruik**: test Ton in de app.

## Documentatie

Bij elke versie: CHANGELOG, ROADMAP, PROJECT.md (Mixer, Bewerken, Laden, projectformaat) en de gebruikershandleiding in `docs/manual/`.
