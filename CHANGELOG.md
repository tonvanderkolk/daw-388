# Changelog

Alle noemenswaardige wijzigingen in DAW 388. Versienummers volgen [Semantic Versioning](https://semver.org/lang/nl/).

## [Unreleased]

### Gewijzigd
- CI: bij een gewone push alleen een controle op Windows (opmaak, Clippy, tests). De release voor macOS en Windows, met downloadbare apps, wordt gebouwd bij een versielabel of een handmatige start. Dat spaart build-minuten.

## [0.21.0] - 2026-10-29

### Gewijzigd
- De console staat op een vast raster, zoals de 388: elke knop en toets op een vaste plek, de labels ernaast zonder iets te verschuiven.
- De strips hebben de indeling van de 388: vaste vlakken onder elkaar (INPUT, EQ, AUX, ASSIGN, PAN) met dunne lijnen ertussen, de knoppen recht onder elkaar op de middenlijn, de ASSIGN-toetsen in één kolom met hun nummers ervoor, het OL-lampje bij TRIM en het nummer onder de fader.
- De master-sectie heeft twee kolommen: EFFECT RTN met plate, echo en de MONITOR-sectie (en de PGM MASTERS), en MASTER met AUX/EFF, de meter-toetsen, luisteren met PHONES en de STEREO MASTER. Alle faders staan op één lijn.
- Het bedieningspaneel staat op de vijf kolommen van de transporttoetsen: RESET, CUE, STC, REPEAT en RTZ, daaronder INSERT, PUNCH, CLICK en TAP, en onder het transport PITCH, VARI, FIX, TAPE en EXPORT, met tempo en maat op één regel.
- Keuzelijsten, het tempoveld en de menu's hebben dezelfde lettergrootte als de labels van de console.
- De meterbrug is even breed als de console, met de naamplaat aan de rechterkant; de PGM MASTERS staan naast elkaar in het midden van hun kolom.

### Toegevoegd
- Dubbelklik op de naam onder de fader om een spoor een andere naam te geven, zoals in SPOREN.
- Volledig scherm aan en uit met Ctrl+Cmd+F (F11 op Windows), via Bestand, Volledig scherm, of met het icoontje rechts in de kopregel.
- Het venster wordt bij het starten en na een wissel tussen Eenvoudig en 388 even breed als de console; overgebleven ruimte (volledig scherm) valt gelijk aan beide kanten.

## [0.20.0] - 2026-10-29

### Gewijzigd
- Toetsen zoals op de 388: verhoogd, in de kleur van hun functie, met het label op het paneel erboven en een led voor de stand. Het transport heeft symbolen (◁◁ ▷▷ □ ▷ en een rode stip voor REC).
- MIC / LINE / RMX bovenaan elke strip is een schuifschakelaar.
- Keuzelijsten en invoervelden in de kleuren van de console.

## [0.19.0] - 2026-10-29

### Toegevoegd
- Meterbrug met VU-meters: TRK 1-8 en twee grote meters voor STEREO L/R of AUX en EFFECT (de METER-schakelaar). De naalden lezen als een echte VU (gemiddeld, ongeveer 300 ms, 0 VU = -18 dBFS); het OL-lampje van een spoor zit in zijn meter.
- Naamplaat "DAW 388 / STUDIO 8".
- Bandrecorder met spoelen achter rookglas, die met de band meedraaien bij afspelen en spoelen; de wikkel verschuift van links naar rechts. Erboven de tape-regel, bijvoorbeeld "456-stijl · 7½ IPS · dbx aan".
- Lettertype IBM Plex Sans Condensed en IBM Plex Mono (SIL Open Font License).

### Gewijzigd
- Twee weergaven in plaats van de la: MIXER met de hele console en SPOREN met de sporen over het hele venster, onder de VU-brug, de teller en de transporttoetsen. Wisselen met de armsteun of Tab.
- In SPOREN per spoor een kleine fader voor het volume en een PAN-knop.
- De UI schaalt mee met het venster (50% tot 125%), zodat de weergave altijd helemaal past.
- Alle faders staan op één hoogte, ook die van de master-sectie.
- De balkmeters in de kanaalstrips en in de master-sectie zijn vervangen door de meterbrug. De TRK-balkjes van de MONITOR-sectie blijven.

## [0.18.0] - 2026-10-28

### Toegevoegd
- De la met de sporen: klik op de leren armsteun onder de mixer om hem uit of in te schuiven, sleep de armsteun voor de hoogte. Per spoor de regio's met hun golfvorm, een tijdlijnbalk met maten of minuten en de afspeelpositie; tijdens het afspelen bladert de la mee.
- Regio's selecteren (Shift voor meer), verschuiven (ook naar een ander spoor), aan een rand inkorten of terugtrekken tot het einde van de opname, splitsen op de afspeelpositie (S), knippen, kopiëren, plakken en wissen.
- Een tijdvak over alle sporen slepen in de tijdlijnbalk: knippen is een splice (de rest schuift aan), plakken voegt in.
- RASTER: vastklikken op de tellen van de metronoom en op randen, nulpunt, CUE-punt en afspeelpositie; Alt ingedrukt is tijdelijk vrij.
- Zoomen met Cmd en scrollen of met −, PAST en +; scrollen schuift door de tijd.
- Bestanden uit de Finder in de la slepen: ze komen op het laatst aangeklikte spoor (en de sporen erna) vanaf de afspeelpositie, samen één undo-stap.
- Dubbelklik op de naam van een spoor in de la om hem te wijzigen.
- De stand van de la (open, hoogte, zoom en RASTER) wordt per project bewaard.

## [0.17.0] - 2026-10-28

### Toegevoegd
- Sporen bestaan uit regio's: stukken van een audiobestand op een plek op de band. Wat op een plek komt, vervangt wat er lag; de bestanden blijven bewaard.
- Laad zet een bestand als regio op de afspeelpositie en vervangt alleen dat stuk.
- Cmd+Shift+Z (Bewerken, Opnieuw) doet een ongedaan gemaakte stap opnieuw. Undo en redo gelden voor opnames, laden, importeren en Wis, tot 10 stappen.

### Gewijzigd
- Een opname wordt een eigen bestand en een regio; het hele spoor wordt niet meer opnieuw geschreven.
- Projectformaat 3 met regio's per spoor. Projecten van eerdere versies openen gewoon.
- Elke regio krijgt een fade van 3 ms aan een vrije rand en een crossfade waar twee regio's elkaar raken, zodat een knip nooit tikt. Daardoor hebben ook begin en einde van sporen uit een ouder project nu zo'n korte fade.

## [0.16.0] - 2026-10-28

### Toegevoegd
- Import van FLAC en MP3, naast WAV en AIFF. Het formaat wordt herkend aan de inhoud, niet aan de naam.

## [0.15.0] - 2026-10-28

### Toegevoegd
- Export (knop EXPORT of Bestand, Exporteren): een stereo bounce en losse sporen als WAV of AIFF, 16 bit (met dither), 24 bit of 32 bit float, 44,1, 48 of 96 kHz. Offline gerenderd, sneller dan realtime, met uitklinkende galm en echo.
- Losse sporen met mixer en tape (stereo, met de eigen galm en echo; samen precies de mix), alleen tape (mono) of droog (de opname zelf).
- Het hele nummer, of van het nulpunt tot het CUE-punt, eventueel met stilte ervoor zodat de bestanden op de band passen.
- Samplerate-conversie voor de export met `rubato`.

## [0.14.0] - 2026-10-28

### Toegevoegd
- dbx per groep (sporen 1-4 en 5-8) in het venster Tape, standaard aan. Codering en decodering rond de band, zodat de ruis in stiltes wegzakt en onder de muziek meeademt.

### Gewijzigd
- Bandruis zonder dbx is nu -58 dBFS bij 100%, zoals een machine zonder ruisonderdrukking. Met dbx aan is de ruis in stiltes zachter dan in 0.13.0 en onder muziek iets hoorbaarder.

## [0.13.0] - 2026-10-28

### Toegevoegd
- Tape-model bij afspelen: bandsnelheid 3¾, 7½ of 15 IPS, tapesoort 456, 250 of 911, en SATURATIE, WOW & FLUTTER, RUIS en CROSSTALK van 0 tot 200%. Opnames via de ingang blijven schoon.
- Schakelaar Clean / 388 en een venster Tape, te openen met de knop TAPE bij de transportsectie of via Bestand, Tape-instellingen.

### Gewijzigd
- Een nieuw project speelt af via de 388-band (7½ IPS, 456). Een project van 0.12.0 of ouder opent op Clean en klinkt zoals het gemixt is.

## [0.12.0] - 2026-10-28

### Toegevoegd
- Ingebouwde plate reverb op AUX en tape echo op EFF, terug op de stereobus via EFFECT RTN 1 en 2 (GAIN en PAN), zoals op de 388.
- Plate: DECAY, PRE-DELAY en TONE. Echo: TIME (vrij of SYNC 1/4, 1/8, 1/8 gepunteerd op het tempo), FEEDBACK en TONE. TIME glijdt zoals een bandmotor; de herhalingen worden donkerder en een tikje verzadigd.
- SOLO haalt een spoor uit de effecten, maar niet uit de AUX-uitgang naar de interface.

### Gewijzigd
- Een project van 0.11.0 of ouder opent met de RTN's dicht, zodat een koptelefoonmix op AUX niet ineens galm geeft. Een nieuw project heeft de RTN's open.

## [0.11.0] - 2026-10-28

### Toegevoegd
- AUX-send met PRE/POST en EFF-send per kanaal, in beide standen. PRE hangt niet af van EQ, fader en MUTE: handig voor een koptelefoonmix.
- AUX MASTER en EFF MASTER, elk naar een vrije uitgang van de interface (Instellingen, Audio).
- STEREO, AUX of EFF op de hoofduitgang, en MONITOR/PHONES voor het niveau. De klik houdt zijn eigen volume.
- METER-schakelaar: de stereometers tonen L-R of AUX en EFF.

### Gewijzigd
- SOLO raakt de sends niet, zodat een koptelefoonmix blijft staan.
- Van 388 naar Eenvoudig laat de sends, de AUX- en EFF-masters en PHONES staan.
- Compactere REC-, MUTE- en SOLO-knoppen en een iets breder venster bij de start, zodat de transportsectie in beeld past.
- EFFECT RTN komt met de effecten in fase 6.

## [0.10.1] - 2026-10-28

### Toegevoegd
- SOLO per spoor, naast MUTE, in beide standen. Zolang er een spoor gesoloed is, hoor je alleen de gesoloode sporen. Opnames, bussen en pingpong merken er niets van. SOLO wordt niet in het project bewaard.

## [0.10.0] - 2026-10-28

### Toegevoegd
- Eenvoudige stand, standaard voor nieuwe projecten. Per spoor een ingang (MIC of LINE), TRIM, EQ, PAN, REC en fader. Zolang een spoor op REC staat hoor je de ingang, anders de band. De opname krijgt geen EQ; die hoor je bij meeluisteren en afspelen.
- Schakelaar Eenvoudig | 388 in de bovenbalk. Naar 388 verandert niets aan het geluid; naar Eenvoudig vraagt de app eerst als de 388-routering daarbij verdwijnt.
- AUTO per kanaal in de 388-stand: de band, en de ingang zodra het spoor op REC staat (met INSERT vanaf het inprikpunt).

### Gewijzigd
- Een project van 0.7.0 of ouder opent nu als eenvoudig project, met de oude fader op de kanaalfader. Projecten van 0.8.0 en 0.9.0 openen in de 388-stand, zoals ze gemaakt zijn.
- Grotere EQ-knoppen en een iets bredere kanaalstrook.
- De sends en masters schuiven op naar 0.11.0.

## [0.9.0] - 2026-10-28

### Toegevoegd
- 3-band EQ per kanaal: LOW 50 Hz tot 1 kHz, MID 500 Hz tot 5 kHz en HIGH 2,5 tot 15 kHz, elk ±15 dB. Elke band is een concentrische knop zoals op de 388: slepen in het midden regelt de gain, op de ring de frequentie. Dubbelklik zet het aangeklikte deel terug. Een vlakke EQ laat het signaal ongemoeid.
- MIC gaat door een nagebootste voorversterker die boven ongeveer -6 dBFS zacht en licht asymmetrisch verzadigt en onder 0 dBFS blijft. LINE en RMX blijven schoon.
- OL-lampje per kanaal: aan bij een piek na de EQ boven -3 dBFS, een halve seconde nagloeiend.

### Gewijzigd
- DIRECT en de PRE-kanaalmeter liggen nu na de EQ. Een project van 0.8.0 opent met een vlakke EQ.

## [0.8.0] - 2026-10-28

### Toegevoegd
- Mixer naar het blokschema van de 388. Per kanaal INPUT MIC, LINE of RMX (de band), TRIM, een PAN-knop en ASSIGN 1-2, 3-4, 5-6 en 7-8 naar de PGM-bussen: PAN links gaat naar de oneven bus, rechts naar de even bus.
- PGM MASTERS ODD en EVEN. Spoor N neemt PGM-bus N op, dus ping-pongen van sporen naar een vrij spoor kan nu.
- DIRECT per spoor: het spoor neemt zijn eigen kanaal op (na TRIM, voor de fader), zonder bus.
- MONITOR-sectie met per spoor GAIN, PAN, mute en een TRK/PGM-meter. Wat je hoort volgt de omschakeltabel van de 388: MON op CUE geeft band of ingang volgens REC FUNCTION en INSERT, op PGM altijd de bussen.
- L-R per kanaal naar de stereobus, voor de remix. Staat er een kanaal op L-R, dan hoor je alleen de L-R-kanalen.
- MONO-schakelaar bij de STEREO MASTER.
- Opnemen kan nu ook zonder ingangsapparaat, van band naar band. Zo'n bounce ligt sample-precies onder de bron; alleen opnames van een ingang krijgen latencycompensatie.
- PROJECT.md legt de routing uit met een schema en recepten voor opnemen, pingpong in mono en stereo en de mixdown.

### Gewijzigd
- `project.json` heeft formaatversie 2. Een project van een eerdere versie gaat open zoals het klonk: de oude spoorfader, pan en mute staan nu in de MONITOR-sectie, de kanaalfader staat op 0 dB. Een spoor met een ingang (in oude projecten is dat elk spoor) wordt een LINE-kanaal met DIRECT, zodat opnemen werkt als voorheen. Daardoor tonen de kanaalmeters na de omzetting de ingang in plaats van de band; de TRK/PGM-meters in de MONITOR-sectie tonen wat je hoort. Zet een kanaal op RMX om de band weer op de kanaalmeter te zien.
- Een spoor op REC FUNCTION met INSERT aan is stil zolang de band stilstaat, zoals op de 388. Zet MON op PGM om dan de ingang te horen.
- REC weigert niet meer zonder ingang, maar waarschuwt als een klaargezet spoor niets binnenkrijgt.
- De ingang kies je nu per kanaal (voor MIC en LINE), in de kanaalstrook of in de instellingen. Het venster is hoger en scrolt ook verticaal.

## [0.7.0] - 2026-10-27

### Toegevoegd
- Metronoom: CLICK-knop met een lampje dat op de tel meeknippert (sneltoets K), tempo in BPM, TAP voor tap tempo en een keuze uit maatsoorten (2/4 tot 7/4, 3/8, 6/8 en 7/8).
- Een hogere klik op tel 1 van elke maat. De klik is alleen te horen: hij komt niet in de mixmeters en nooit in een opname.
- Aftellen van 1 of 2 maten bij REC vanuit stop; de band wacht en de opname begint daarna precies op de tel.
- In de instellingen: klikvolume, aftellen en of de klik ook tijdens gewoon afspelen klinkt.
- De teller toont maat en tel, en tijdens het aftellen "AFTELLEN".
- Alle metronoominstellingen worden per project bewaard.

## [0.6.1] - 2026-10-27

### Opgelost
- De Windows-build faalde op de codecontrole (Clippy): de audiostreams zijn nu zo opgeslagen dat ze ook onder Windows klein blijven.

## [0.6.0] - 2026-10-27

### Toegevoegd
- Opnemen. Ingangsapparaat kiezen in de instellingen (alle kanalen van de interface) en per spoor de ingang.
- REC FUNCTION per spoor (knippert klaar, brandt tijdens opname) en een REC-toets bij het transport, met R als sneltoets. REC vanuit stop start ook de band, zoals REC en PLAY samen op de 388.
- Punch-in met REC tijdens het afspelen, of met REC FUNCTION in record-ready. Punch-out met PLAY (speelt door), STOP of de spoeltoetsen, of automatisch bij het CUE-punt.
- INSERT: klaargezette sporen spelen de band tot de punch-in en schakelen dan naar de ingang.
- AUTO PUNCH: opnemen van het nulpunt tot het CUE-punt, met REPEAT bij elke doorgang opnieuw.
- Meeluisteren via DAW 388 (standaard) of via de interface (direct monitoring).
- Latencycompensatie op basis van de gemeten apparaatlatency, met een handmatige correctie in milliseconden.
- Opnames worden 24-bit WAV in de projectmap. Een punch-in vervangt alleen het opgenomen stuk, zoals op band.
- Menu Bewerken met Opname ongedaan maken (Cmd+Z), tot 10 opnames terug.

### Gewijzigd
- Het venster is iets groter om ruimte te maken voor de opnameknoppen.

## [0.5.0] - 2026-10-27

### Toegevoegd
- Audio-instellingen in het instellingenvenster: uitvoerapparaat, samplerate (44,1 tot 192 kHz, voor zover het apparaat die biedt) en buffergrootte (32 tot 2048 samples), met de latency in milliseconden.
- De app onthoudt de gekozen uitvoer tussen het opstarten (`settings.json` in de configuratiemap).
- Valt het apparaat weg, dan schakelt de app meteen over op het standaardapparaat, op dezelfde plek, en terug zodra het gekozen apparaat er weer is.
- Speelde de band, dan speelt hij na het omschakelen van apparaat, samplerate of buffer gewoon door.
- Stille klok: zonder enige audio-uitvoer blijven transport, teller en meters gewoon werken.

### Gewijzigd
- Een gekozen apparaat dat ontbreekt geeft nu een duidelijke melding ("... niet gevonden, nu via ...") in plaats van stil het standaardapparaat te gebruiken.

## [0.4.0] - 2026-10-27

### Toegevoegd
- Transport naar het voorbeeld van de 388: RESET (nulpunt), CUE (geheugenpunt), STC (naar het CUE-punt), REPEAT (herhalen van nulpunt tot CUE-punt), REW en F FWD (stil spoelen op 30 keer de snelheid), STOP, PLAY en RTZ (naar het nulpunt).
- Teller vanaf het nulpunt, met een minteken ervoor, en het CUE-punt eronder.
- PITCH-knop: snelheid en toonhoogte plus of min 15%, met VARI/FIX-schakelaar. Dubbelklik zet hem op 0%.
- Instellingenvenster (Bestand, Instellingen of Cmd+,) met de REPEAT-modus: 388 (de band spoelt in de tijd terug, zoals het origineel) of cycle (springt direct terug, zoals in Logic).
- Nulpunt, CUE-punt, REPEAT-modus, pitch en VARI/FIX worden in het project opgeslagen.
- Sneltoetsen: komma is REW, punt is F FWD, C zet REPEAT aan of uit.
- Bestanden en projecten laden op de achtergrond; het spoor toont "laden..." tot het klaar is.

### Gewijzigd
- RTZ gaat naar het nulpunt (RESET) in plaats van altijd naar het begin. In de 388-modus spoelt RTZ in de tijd.

## [0.3.0] - 2026-10-27

### Toegevoegd
- Schakelaar TRACK METERS PRE/POST in de master-sectie. PRE (standaard, zoals op de 388) toont het niveau van het spoor zelf, POST het niveau na fader en mute. De stand wordt in het project opgeslagen; oudere projecten openen met PRE.

### Gewijzigd
- Fader-curve naar het voorbeeld van Logic Pro: +6 dB bovenaan, 0 dB op ongeveer driekwart en rond 0 dB ongeveer 11 punten per dB (met Shift 55). Schaal 6, 3, 0, -3, -6, -10, -15, -20, -30, -40 en ∞. Een fader die in een eerder project hoger dan +6 dB stond, opent op +6 dB.
- Meters staan rechts van de fader en hebben een eigen schaal zoals in Logic: 0 dBFS bovenaan, gelijke stappen tot -24 en daaronder samengedrukt tot -60.

## [0.2.2] - 2026-10-27

### Gewijzigd
- Faders hebben een mengtafel-curve: 0 dB zit op ongeveer driekwart van de baan en tussen -10 en +5 dB is veel ruimte (ongeveer 6 punten per dB, met Shift 30). De stille kant onderin is samengedrukt.
- Langere faderbaan (260 punten) met schaalverdeling (+10 tot -40 en ∞) naast elke fader.
- Een dichte fader toont "-∞ dB".

## [0.2.1] - 2026-10-27

### Toegevoegd
- Import van AIFF en AIFF-C (`.aif`, `.aiff`, `.aifc`): 8, 16, 24 en 32-bit integer en 32-bit float, mono en stereo, elke samplerate.
- Het formaat wordt herkend aan de inhoud van het bestand, dus een verkeerde extensie is geen probleem.

### Gewijzigd
- De bestandskiezer toont WAV- en AIFF-bestanden; de menuknop heet nu "8 audiobestanden laden".

## [0.2.0] - 2026-10-26

### Toegevoegd
- Projecten opslaan en openen: een projectmap met `project.json` (fader, pan en mute per spoor, master) en een submap `audio/`. Bij het opslaan worden de WAV-bestanden naar `audio/` gekopieerd, zodat de projectmap op zichzelf staat.
- Menu Bestand met Nieuw, Openen, Opslaan en Opslaan als, plus de sneltoetsen Cmd+N, Cmd+O, Cmd+S en Shift+Cmd+S (Ctrl op Windows).
- Autosave 30 seconden na een wijziging, zodra het project een map heeft. Bovenin staat "Opgeslagen" of "Automatisch opgeslagen" met de tijd.
- Backups van `project.json` in `backups/`: bij elke handmatige opslag en hooguit elke 5 minuten bij autosave. De laatste 10 blijven bewaard.
- Vraag om op te slaan bij Nieuw, Openen, Project sluiten, Cmd+Q of het sluiten van het venster als er niet-opgeslagen wijzigingen zijn.
- Project sluiten (Cmd+W) en Stop DAW 388 (Cmd+Q) in het menu Bestand. Op macOS liep Cmd+Q eerst buiten de opslaan-vraag om; het standaard app-menu van macOS is daarom uitgezet.
- Projectnaam in de titelbalk, met een sterretje bij niet-opgeslagen wijzigingen.
- Ontbrekende audiobestanden worden per spoor in rood gemeld en blijven in het project staan, zodat het terugzetten van het bestand het project herstelt.
- Een projectmap of `project.json` kan als argument worden meegegeven.

### Gewijzigd
- CI: tests draaien in het release-profiel, zodat de release-build dezelfde compilatie hergebruikt. Dat scheelt vooral op Windows veel tijd.
- CI: `actions/checkout` en `actions/upload-artifact` bijgewerkt naar v7 (Node.js 24), wat de waarschuwing over Node.js 20 oplost.

## [0.1.1] - 2026-10-26

### Opgelost
- Tikjes bij het verschuiven van de stereo master-fader: de master-gain verandert nu geleidelijk, net als de spoorfaders.
- Een dichtgetrokken fader geeft nu echt stilte.

## [0.1.0] - 2026-10-26

### Toegevoegd
- Cargo workspace met de crates engine, tape, fx, project, ui en app-desktop.
- Audio-engine met 8 mono sporen, transport (PLAY, STOP, RTZ, locate) en een stereomix met gain, constant-power pan, mute en master, plus piekmeters per spoor en voor de stereo-uitgang.
- Lock-free communicatie tussen UI en audiothread; buffers worden alleen op de UI-thread vrijgegeven.
- WAV-import (8 tot en met 32-bit integer en 32-bit float), mixdown naar mono en samplerate-conversie naar het apparaat.
- Desktop-app (macOS en Windows) met keuze van het audio-uitvoerapparaat en terugval op het standaardapparaat.
- Venster met 8 kanaalstrips (laden, wissen, pan, mute, fader, meter), stereo master, LED-teller en transporttoetsen. Sneltoetsen spatie en Home.
- WAV-bestanden kunnen als argument worden meegegeven.
- GitHub Actions-workflow die test en bouwt voor macOS en Windows.
- PROJECT.md, ROADMAP.md, README.md en licenties (MIT OR Apache-2.0).
