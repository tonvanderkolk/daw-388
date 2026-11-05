# Mixdown deck (0.36.0)

## Waarom

Een eindmix ging vroeger niet terug op de 388. Alle acht sporen waren nodig om hem af te spelen, dus de STEREO-uitgang ging naar een tweede machine: een 1/4 inch tweesporenrecorder, een cassettedeck of later een DAT. DAW 388 heeft alleen de export, die de mix in één keer uitrekent. Omdat er geen automatisering is, gaan je live bewegingen (faders, galm, echo, MUTE) dan verloren. Wie ze wil bewaren, moet nu twee sporen opofferen met Bounce Setup.

Het mixdown deck is die tweede machine. Het neemt de STEREO MASTER live op met alles wat je onderweg doet en geeft de mix een tweede laag bandkarakter. Het kost geen spoor.

Bounce Setup en TO PGM blijven: die zijn voor het pingpongen, het samenvoegen van sporen om er weer vrij te maken. De handleiding maakt dat onderscheid duidelijk.

## Wat de gebruiker ziet

Het deck staat in het lege vak rechtsonder in de MIXER, onder het transport. Bovenaan staat MIXDOWN. Het vak bevat:

- **Twee kleine spoelen** die draaien terwijl het deck opneemt of afspeelt.
- **Een keuze van de machine**, een LED-keuzelijst zoals TO PGM:
  - `1/4" 15 IPS`: een tweesporenrecorder op 15 ips. Warm en vrij transparant, weinig ruis, de kop-bult rond 100 Hz en zachte verzadiging.
  - `CASSETTE`: meer ruis (met ruisonderdrukking zoals Dolby B, die alleen het hoog terugdraait), minder hoog (vanaf ongeveer 12 kHz), meer wow en flutter en eerder verzadigd. De demosound.
  - `DIRECT`: digitaal en schoon, zonder extra band.
- **De toets MIX REC** met een rood lampje. Staat hij aan, dan neemt het deck op zolang het transport speelt. Elke keer van PLAY tot STOP wordt één take.
- **Een vinkje Keep clean copy**, standaard uit. Staat het aan, dan bewaart het deck naast de take door de band ook de schone mix. Die kost dubbel zoveel ruimte.
- **De lijst van mixen**: "Mix 1", "Mix 2" en verder, de nieuwste bovenaan, met de lengte en de machine erbij ("Mix 3 · 3:42 · CASSETTE"). Per mix:
  - ▶ en ■ om hem af te spelen;
  - dubbelklik op de naam om hem een andere naam te geven;
  - Save... om hem als bestand te bewaren;
  - Print through... (alleen met een schone kopie) om hem opnieuw door een andere machine te halen, wat een nieuwe mix geeft;
  - een prullenbak om hem uit de lijst te halen.

In de compacte console is er geen ruimte voor het vak. Daar opent View, Mixdown Deck... hetzelfde als een venster.

## Luisteren

Zolang MIX REC aan staat, hoor je op LISTEN STEREO de mix door het deck: van de band af, zoals bij een machine met drie koppen. Wat je hoort, is wat er in de take komt. Staat MIX REC uit, dan hoor je de console zoals nu.

Afspelen van een mix stopt het transport van de 388 en speelt de mix op de hoofduitgang, na PHONES. De VU's STEREO L en R tonen hem. Tijdens het afspelen van een mix kun je niet opnemen.

## Waar het signaal vandaan komt

Het deck neemt op na de STEREO MASTER en MONO, op hetzelfde punt waar de grote meters en het OL-lampje kijken, voor LISTEN en PHONES. De metronoom komt er niet op, ook niet als hij hoorbaar is. Een take door de band is dus de export met live bewegingen erin, plus het deck.

## Bewaren

- Een take is een stereo-WAV van 32 bit float op de samplerate van de engine, in een nieuwe map `mixes/` in het projectpakket. Een schone kopie staat ernaast, met dezelfde naam en `clean` erachter.
- `project.json` krijgt een lijst `mixes` (serde default leeg), per mix: de naam, het bestand, de schone kopie als die er is, de machine en de lengte.
- Uit de lijst gehaald gaat een mix niet direct weg: zijn bestanden gaan bij het sluiten van het project naar de prullenbak, zoals ongebruikte opnames nu. Dat is één undo-stap, dus terughalen kan.
- Save... gebruikt de map, het formaat (WAV of AIFF) en de bitdiepte van het exportvenster, met de bestandsnaam `Project_Mix 3`. Daarna staat de piek erbij, zoals bij een export.

## De export

Het exportvenster krijgt bij de stereomix de keuze Through: `DIRECT`, `1/4" 15 IPS` of `CASSETTE`, standaard `DIRECT`. Dan kun je ook zonder live opname een mix met bandkarakter maken. Losse sporen gaan altijd direct.

## Techniek

- **De machines** komen in het crate `tape`, als een `MixdownDeck` met twee kanalen. Het hergebruikt de bouwstenen van de 388 (de verzadigingscurve, de filters, wow en flutter, de ruis). Per machine zijn de instellingen vast; er zijn geen knoppen. De ruis is per kanaal onafhankelijk; wow en flutter zijn voor beide kanalen gelijk, want het is één band.
- **De engine** krijgt het deck achter de master: `Command::SetDeck { machine, rec }`. Tijdens een opname gaan de samples door een eigen ringbuffer (rtrb) naar een `MixWriter` in de UI, net als de takes van de sporen nu. De schone kopie gaat mee als tweede paar kanalen in dezelfde stroom.
- **Afspelen** van een mix gaat via een eigen bron in de engine. Dat zijn dezelfde bouwstenen als het laden van een spoor, maar in stereo en zonder band.
- **De export** haalt de stereomix na het renderen door hetzelfde `MixdownDeck`, zodat live en offline gelijk klinken.

## Tests

- Het deck op `DIRECT` laat het signaal bit voor bit door.
- `CASSETTE` heeft minder hoog dan `1/4" 15 IPS` en die minder dan `DIRECT`; de ruis van `CASSETTE` is hoger.
- Wow en flutter zijn gelijk op beide kanalen.
- Een opname van PLAY tot STOP geeft één take met de juiste lengte. Met Keep clean copy is er ook een schone kopie, gelijk aan de master.
- De metronoom komt niet in de take.
- `project.json` met en zonder `mixes` laadt; een mix uit de lijst halen is één undo-stap en zijn bestand gaat bij het sluiten naar de prullenbak.
- Een export met Through `CASSETTE` klinkt als een take met `CASSETTE` (zelfde verwerking, op de wow na die met de tijd loopt).

## Buiten de scope

- Knippen en plakken op de mastertape (de beste stukken van mixen aan elkaar, stiltes tussen nummers). Komt op de ROADMAP.
- DAT en de halve inch op 30 ips.
- Knoppen per machine (ruis, verzadiging); de vaste profielen zijn genoeg om mee te beginnen.

## Documentatie

- De handleiding krijgt een hoofdstuk Mixdown deck. Bij Bounce Setup komt te staan dat het voor pingpongen is en het deck voor de eindmix.
- CHANGELOG, ROADMAP, PROJECT.md (het signaalpad en het formaat), CLAUDE.md en de versie in Cargo.toml naar 0.36.0.
