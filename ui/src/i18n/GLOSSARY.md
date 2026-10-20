# Translation glossary

The words the interface and the help reference use for PolarExplorer' own
concepts, fixed so every area — and every later feature — says the same
thing. The audience is navigators, routers and performance analysts: prefer
the word a French or German sailor, a class rule or a routing program would
use, not a dictionary's.

Copied in form from VectorEffects' glossary. The translations were written by
the implementer (plan.md M2, completed in M17b). **Review status of every
entry: machine-drafted, needs a native sailor.** No native-speaking sailor was
available (M17 preflight ruling); `docs/TRANSLATION-REVIEW.md` is the checklist
for the human review. Entries marked **⚑** are the ones the implementer was
least sure of; each says why in its note.

**Register.** French addresses the reader as *vous*; German as *Sie*. Button
and menu labels are infinitives in French ("Enregistrer") and infinitives or
nouns in German ("Speichern", "Einstellungen").

**Punctuation.** French puts a narrow no-break space (U+202F) before
`: ; ? !` and uses « » with no-break spaces inside; German uses „ “. Keep `…`
where the English has it (it means "opens a dialog"). Keep every
`{placeholder}` exactly as written.

**Never translated:** PolarExplorer, ORC, TWA, TWS, BSP, VMG, Expedition,
Adrena, YellowBrick, Geovoile, Blue Water Tracks, GeoJSON, CSV, GRIB, ERA5,
WebGL2, file extensions (.wpsproj), unit symbols (kn, m/s, km/h, m, ft, nm,
km, GB, s, °).

**Menus** follow each platform's own wording in that language (macOS:
"Quitter", "Widerrufen", "Einsetzen" would be macOS German; we use the
cross-platform "Einfügen"). The native menu's labels live in Rust,
`crates/pe-app/src/menu.rs`, and must agree with this table.

| English | Français | Deutsch | Note |
|---|---|---|---|
| polar | polaire (f.) | Polare (f.) | Never "diagramme polaire" for the data itself. |
| polar plot / polar diagram | diagramme polaire | Polardiagramm | The 2D drawing. |
| blend (noun) | fusion | Mischung | The exported result of the enabled sources. ⚑ Plain words, not a routing-software term; a reviewer may prefer "polaire combinée" / "kombinierte Polare". |
| blend (verb) | fusionner | zusammenführen | ⚑ "einmischen" (to meddle) was wrong; "zusammenführen" is also the word for the global *merged* current, which a reviewer may want kept apart. |
| source | source | Quelle | Anything that contributes to the polar. |
| weight (blend weight) | poids | Gewichtung | |
| output grid | grille de sortie | Ausgaberaster | The TWA × TWS grid the blend is made on (M14). |
| coverage (of the blend) | couverture | Abdeckung | "direct" / "filled": directe / remplie, direkt / gefüllt. |
| confidence (full confidence) | confiance (pleine) | Vertrauen (volles) | A track cell's weight by its samples. |
| export (verb / noun) | exporter / export | exportieren / Export | |
| Blend settings | Réglages de la fusion | Mischungseinstellungen | |
| overlay | surcouche | Überlagerung | A user change stored beside a source. |
| track | trace | Track | German sailors say "Track"; not "Spur". |
| tracker | balise | Tracker | The position-reporting device and service. |
| race | course | Regatta | |
| sample (a track position) | point | Punkt | Shown as a dot; "échantillon"/"Messpunkt" only in technical text. |
| polar segment | segment de polaire | Polarsegment | |
| polar node (a grid point of a polar) | nœud de polaire | Polarknoten | "nœud de grille" / "Gitterpunkt" in descriptions. |
| exclude / include (from the blend) | exclure / inclure | ausschließen / aufnehmen | |
| surface (3D) | surface | Fläche | |
| polar tower (3D layout) | tour polaire | Polarturm | |
| ORC certificate | certificat ORC | ORC-Messbrief | "Messbrief" is the class-rule word. |
| sister ship | sistership | Schwesterschiff | |
| TWA — true wind angle | angle du vent réel | wahrer Windwinkel | Keep "TWA" as the abbreviation; *le* TWA, *der* TWA. |
| TWS — true wind speed | vitesse du vent réel | wahre Windgeschwindigkeit | Keep "TWS"; *le* TWS, *die* TWS. |
| BSP — boat speed | vitesse du bateau | Bootsgeschwindigkeit | Keep "BSP"; *la* BSP, *die* BSP. ⚑ French routers also say "vitesse surface". |
| VMG | VMG | VMG | |
| heading | cap | Kurs | |
| tack (side) | amure | Bug | "tribord amure" / "Steuerbordbug". |
| to tack (manoeuvre) | virer de bord | wenden | Noun: virement / Wende. |
| to gybe | empanner | halsen | Noun: empannage / Halse. |
| to bear away | abattre | abfallen | Noun: abattée / Abfallen. |
| to luff up | lofer | anluven | |
| upwind / downwind | au près / au portant | am Wind / vor dem Wind | |
| wind / waves / current | vent / vagues / courant | Wind / Wellen / Strömung | Current is "toward", wind and waves "from". |
| wave height | hauteur des vagues | Wellenhöhe | Significant height (ERA5's): hauteur significative / signifikante Wellenhöhe. |
| reanalysis | réanalyse | Reanalyse | |
| chunk cache | cache de blocs | Datenblock-Cache | Downloaded reanalysis data. |
| knots | nœuds | Knoten | Symbol "kn" in every language. |
| nautical miles | milles nautiques | Seemeilen | Symbol "nm". |
| project | projet | Projekt | |
| start screen | écran d'accueil | Startbildschirm | |
| stage (Map / 3D / Compare) | vue | Ansicht | |
| Map / 3D / Compare | Carte / 3D / Comparer | Karte / 3D / Vergleich | |
| navigation (left panel) | navigation | Navigation(sleiste) | |
| settings | réglages | Einstellungen | |
| theme | thème | Design | Harbour: Port / Hafen. Midnight: Minuit / Mitternacht. |
| autosave | enregistrement automatique | automatisches Speichern | |
| recovered work | travail récupéré | wiederhergestellte Arbeit | |
| unsaved changes | modifications non enregistrées | ungespeicherte Änderungen | |
| Save / Don't save / Cancel | Enregistrer / Ne pas enregistrer / Annuler | Speichern / Nicht speichern / Abbrechen | |
| Open Recent | Ouvrir un projet récent | Zuletzt geöffnet | |
| equirectangular | équirectangulaire | Plattkarte | |
| orthographic | orthographique | orthografisch | |
| feature search | recherche de commandes | Funktionssuche | |
| control (UI) | commande | Bedienelement | |

## Sailing, meteorological and ORC terms (M17b)

Every such term the interface or the help uses. Review status for all:
machine-drafted, needs a native sailor.

| English | Français | Deutsch | Note |
|---|---|---|---|
| beat angle / run angle (ORC) | angle de près / angle de vent arrière optimaux | optimaler Am-Wind- / Vorwind-Winkel | The ORC certificate's "Beat angle" and "Gybe angle". |
| closer to / further off the wind | plus près du vent / plus abattu | höher am Wind / tiefer | Filter tooltips. ⚑ "tiefer segeln" is the idiom; "raumer" was avoided as too narrow. |
| manoeuvre threshold | seuil de manœuvre | Manöverschwelle | |
| heel | gîte | Krängung | Not shown in v1; fixed here for later features. |
| leeway | dérive | Abdrift | ⚑ French "dérive" also means drift (Stokes drift, current set and drift) and the centreboard; context decides. |
| heading / COG | cap / COG | Kurs / COG | "Cap ou COG", "Kurs oder COG" in the CSV mapping. |
| SOG | SOG | SOG | "vitesse sur le fond" / "Geschwindigkeit über Grund" in prose. |
| over the ground / through the water | par rapport au fond / par rapport à l'eau | über Grund / durchs Wasser | |
| current (toward) | courant (vers) | Strömung (nach) | "{speed} vers {direction}", "{speed} nach {direction}". |
| current set / drift | direction / vitesse du courant | Strömungsrichtung / Strömungsgeschwindigkeit | The app says direction and speed; "set and drift" is not used as such. ⚑ |
| tide, tidal current | marée, courant de marée | Gezeiten, Gezeitenströmung | "currents without tide": courants sans marée / Strömungen ohne Gezeiten. |
| Stokes drift | dérive de Stokes | Stokes-Drift | |
| correct for current | corriger du courant | Strömung herausrechnen | |
| mean wave direction | direction moyenne des vagues | mittlere Wellenrichtung | Waves are "from". |
| head / bow / beam / quarter / following seas | mer de face / par l'avant / de travers / de trois-quarts arrière / de l'arrière | Wellen von vorn / schräg von vorn / von der Seite / schräg von achtern / von achtern | Wave sectors. ⚑ German keywords also carry "querab", "raumschots". |
| angle off the bow | angle depuis l'étrave | Winkel zum Bug | |
| sea state | état de la mer | Seegang | Search keyword. |
| weather (the fetched wind, waves and current) | météo | Wetter | "Fetch weather…": Récupérer la météo… / Wetter abrufen…. |
| environment (wind, waves, current of a sample) | environnement | Umweltdaten | |
| fetch (verb, noun) | récupérer / récupération | abrufen / Abruf | Downloads in general: télécharger / herunterladen. |
| hourly / every 3 hours (sampling) | toutes les heures, pas horaire / toutes les 3 heures | stündlich / alle 3 Stunden | "Abtastung" for sampling. |
| statistic (of a cell) | statistique | Kennwert | 90th percentile: 90e centile / 90. Perzentil; median: médiane / Median; mean: moyenne / Mittelwert. |
| cell (of a polar grid or table) | cellule | Zelle | Not "case" in French: "case" is kept for text boxes ("case de recherche") and tick boxes. |
| grid (output grid, a polar's grid) | grille | Raster | "Gitterpunkt" only for a polar node in German descriptions. |
| slice (2D plot at one TWS) | tranche | Schnitt | |
| dot band (the plot's TWS tolerance) | plage de vent | Windbereich | |
| heat map | carte de chaleur | Heatmap | |
| sail number | numéro de voile | Segelnummer | "n° de voile" in the short placeholder. |
| builder / designer / year built | chantier / architecte / année de construction | Werft / Konstrukteur / Baujahr | ORC fields. |
| class, division | série, division | Klasse, Division | ⚑ "division" in French and German racing is understood; "groupe" / "Gruppe" are alternatives. |
| handicap class | classe de handicap | Handicap-Klasse | |
| VPP | VPP | VPP | "la grille VPP", "das VPP-Gitter". |
| race / leg / fleet | course / étape / flotte | Regatta / Etappe / Flotte | |
| event (a tracked race) | événement | Veranstaltung | Not "Ereignis". |
| race key | clé de course | Regattaschlüssel | YellowBrick's short race name. |
| start / finish | départ / arrivée | Start / Ziel | |
| Racing / Finished / Retired / Did not start / Did not finish | En course / Arrivé / Abandon / Non partant / Non arrivé | Im Rennen / Im Ziel / Aufgegeben / Nicht gestartet / Nicht im Ziel | Tracker status. ⚑ Check against each country's racing-rules wording (DNF, DNS, RET). |
| motoring | moteur | Motorfahrt | "le moteur avant le départ". |
| knots (symbol) | kn | kn | ⚑ French sailors often write "nd" or "nds"; the interface keeps the international "kn" in all languages (it is also the unit picker's symbol). A reviewer may want "nd" throughout French. |

### Interface verbs and nouns fixed in M17b

| English | Français | Deutsch | Note |
|---|---|---|---|
| dialog | fenêtre | Dialog | "fenêtre d'import", "fenêtre de la balise"; not "boîte". |
| tick / untick | cocher / décocher | ankreuzen / abkreuzen | Not "anhaken". |
| leave out (filter) | écarter | weglassen | Not "laisser de côté" / "auslassen". |
| frame (fit on the map) | cadrer | einpassen | Not "formatfüllend zeigen". |
| derive / derived | déduire / déduit | ableiten / abgeleitet | Heading and speed from positions. |
| given (value from the file) | fourni | angegeben | |
| Shift (key) | Maj | Umschalt | In prose, tooltips and chords (`chordText` in `ui/src/chords.ts`). |
| Cmd / Ctrl (accelerator) | ⌘ on a Mac, Ctrl elsewhere | ⌘ on a Mac, Strg elsewhere | English writes Cmd / Ctrl. |
| Compare (a source's button, the verb) | Comparer | Vergleichen | Key `Compare@@verb`; the stage tab is the noun: Comparer / Vergleich. |
| Esc (key) | Échap | Esc | |
| add | ajouter | hinzufügen | Not "addieren" (Stokes drift). |
| undo (as a step) | une annulation | ein Widerrufen-Schritt | "One undo" → "s'annule en une fois" / "ein Widerrufen-Schritt". |
| Hindsight Sync | Hindsight Sync | Hindsight Sync | Product name; never translated. |
| reanalysis archive | archive de réanalyse | Reanalyse-Archiv | The archive chosen in Settings → Reanalysis, listed by Hindsight Sync. |
| Scan (button) | Rechercher | Suchen | Looks for Hindsight Sync; not "Scannen". |
