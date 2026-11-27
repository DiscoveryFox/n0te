package dev.n0bridge.n0te

import android.content.Context

/**
 * What changed in each version, newest first. The newest entry shows once after an install or
 * update (the library, see [unseen]); all of them are in the settings under "Neuigkeiten".
 * A new version gets a new entry at the top: the dialog then shows it by itself.
 */
object Changelog {
    data class Entry(val version: String, val date: String, val title: String, val items: List<String>)

    val all = listOf(
        Entry("0.37.0", "27. Nov. 2026", "Geodreieck, Zirkel und Farbeimer", listOf(
            "Geodreieck, Winkelmesser und Zirkel neben dem Lineal: Lineal-Knopf halten und auswählen. Alles in echter Größe – das Geodreieck mit 16 cm, Millimetern und Gradskala, der Winkelmesser mit 10 cm",
            "Der Stift folgt jeder Kante, auch den Schenkeln des Geodreiecks; ein Finger verschiebt, zwei drehen (Lineal rastet bei 45°, die anderen bei 15°)",
            "Zirkel: Stift aufsetzen für die Nadel (rastet an Strichenden und Schnittpunkten), ziehen für den Radius (in cm angezeigt), dann irgendwo um die Nadel zeichnen; fast ganz herum wird ein voller Kreis",
            "Farbeimer: in eine umrandete Fläche tippen füllt sie durchscheinend mit der aktuellen Farbe; Eimer halten schaltet auf Schraffur. Kleine Lücken im Umriss schließen sich, Schrift darin bleibt frei",
            "Füllungen lassen sich wie Striche auswählen, verschieben, umfärben, radieren und exportieren",
        )),
        Entry("0.36.0", "26. Nov. 2026", "Wegkritzeln, das mitdenkt, ein neuer Laserpointer und klickbare PDFs", listOf(
            "Wegkritzeln schaut, was darunter liegt: Ausmalen und Schraffieren löschen den Umriss nicht mehr (sie bleiben Tinte), ein schnelles Gekritzel über einen einzelnen Buchstaben löscht ihn jetzt. Es geht nur, was wirklich bedeckt ist; einmal Rückgängig holt alles zurück",
            "Neuer Laserpointer: weicher, leuchtender Punkt und eine glatte Spur, die schmaler wird und verblasst – auf allen Geräten gleich",
            "Gäste sehen deinen Laserpointer, auch wenn sie am Laptop hängen, und dürfen selbst zeigen, jeder in seiner Farbe",
            "Links in PDFs: antippen (wenn der Finger nicht schreibt) springt zur Seite, auf die er zeigt, oder öffnet die Webseite; die Zurück-Geste bringt dich zurück",
            "PDFs bringen ihr Inhaltsverzeichnis mit: Kapitel werden beim Import zu Gruppen, einseitige Kapitel zu Seitentiteln (Einstellungen › Seiten)",
            "Favoriten-Stifte: bis zu sechs Stifte mit Farbe, Breite und Linie in der Werkzeugleiste; „+“ merkt den aktuellen, halten entfernt ihn",
            "Gestrichelte, gepunktete und Strich-Punkt-Linien im Stiftmenü; „Linie“ in der Auswahl ändert vorhandene Striche",
            "Was ein Gast wegkritzelt, bleibt beim Gastgeber nicht mehr stehen (und umgekehrt)",
        )),
        Entry("0.35.0", "26. Nov. 2026", "Eigene Designs, Weihnachten und viele Fehler weniger", listOf(
            "Eigene Designs mit KI: Einstellungen › Design › „Mit KI gestalten“ kopiert eine Anleitung für ChatGPT, Claude oder Gemini; die Antwort einfach wieder einfügen. n0te prüft sie, zeigt eine Vorschau und sagt sonst genau, was die KI ändern soll",
            "Acht neue Designs: Weihnachten (Schneefall und Lichterkette), Blaupause, Kreidetafel, Sternennacht, Kirschblüte, Herbst, Terminal und Hoher Kontrast – mit Dekorationen hinter der Bibliothek, nie beim Schreiben, abschaltbar",
            "Designs teilen: dieselbe Datei funktioniert auf dem Tablet, am Laptop und unter Windows",
            "Absturzberichte: nach einem Absturz kannst du einen Bericht (ohne Notizen) kopieren oder teilen",
            "Was du kurz vor dem Verlassen schreibst, wird immer gespeichert; ist eine Seite zweimal offen, gehen keine Striche mehr verloren",
            "Beschädigte Dateien werden vom anderen Gerät repariert; kaputte Importe hinterlassen keine halben Notizbücher",
            "Abstürze behoben: Mitwandern am Seitenrand, Lupe in schmalen Bereichen, Ordner namens „+“, Formeln wie f(x) = f(x) + 1 und mehr",
            "Updates erreichen auch Beta-Tester; PDF-Seiten bleiben nach dem Zoomen nicht mehr weiß",
            "Bibliothek: Mehrfachauswahl wählt die richtigen Notizbücher, Einladungslinks aus Chats funktionieren",
            "Graphen: Zurück behält die Änderungen; Zahlen, Winkel und Kommas werden richtig gelesen; ein Schieberegler ist ein Rückgängig-Schritt",
            "Änderungen vom Laptop erscheinen sofort, auch wenn n0te kurz im Hintergrund war; Seiten mit mehreren Fotos flackern nicht mehr",
            "Beim Drehen bleiben offene Dialoge erhalten, lange Dialoge scrollen, Treffer in zugeklappten Gruppen klappen sie auf",
        )),
        Entry("0.34.1", "26. Nov. 2026", "Gäste bleiben verbunden", listOf(
            "Gäste in mehreren deiner Notizbücher bleiben verbunden: vorher kamen und gingen sie ständig, und was sie schrieben oder an Graphen änderten, kam kaum an",
            "Denselben Einladungslink noch einmal öffnen stört die Verbindung nicht mehr",
            "Als Gast zeigt „Gast bei …“ jetzt richtig, ob der Besitzer erreichbar ist",
            "Neuigkeiten und Update-Dialog: lange Listen scrollen, die Knöpfe bleiben immer sichtbar",
        )),
        Entry("0.34.0", "25. Nov. 2026", "Einzelne Seiten exportieren, riesige Seiten flüssig", listOf(
            "Export: einzelne Seiten wählen, auch nur ein Seitenblatt (z. B. „03 R1“) – auf dem Tablet und am Laptop",
            "Achsen eines Graphen direkt auf der Seite strecken: x- oder y-Achse mit dem Stift ziehen (mit dem Finger, wo Finger schreiben); auf Karopapier rastet sie ein",
            "„a = 3b“ wird auch beim Bearbeiten einer Zeile zur Kurve a(b) = 3b, wie beim Eintippen",
            "Riesige Seiten (z. B. 2 × 2 m) und Boards bleiben flüssig: nur Sichtbares wird gezeichnet, viel Tinte kommt aus Kacheln, lange nicht gesehene Seiten geben Speicher frei",
            "Eigene Seitengrößen bis 5 m pro Seite (vorher 1,4 m)",
            "Am Laptop: Koppeln und Einladungen per Kamera („Mit der Kamera scannen“); große Seiten bleiben auch dort flüssig",
            "n0te Tablet für Windows: Lineal, Platz einfügen, Schreiblupe, Wegkritzeln, „+“ an den Seiten und Boards für sich – wie auf dem Tablet",
        )),
        Entry("0.33.0", "25. Nov. 2026", "Graphen mit Grenzen, schnelle Auswahl", listOf(
            "Definitionsbereiche wie in der Schule: f(x) = x², 0 ≤ x ≤ 2 (auch „für …“ oder in Klammern); die Enden als Punkt oder Kreis, im Graphen ziehbar, in den Einstellungen „von/bis“",
            "Achsen einzeln strecken: an der x- oder y-Achse ziehen ändert nur diese Achse, z. B. y in Hunderterschritten und x in Einern",
            "Mehr Graphen: x² + y² = 4, x·y = 1, (cos t, sin t), r = 1 + cos θ; a = 3b wird als Kurve gezeichnet (Schalter „als Kurve / als Schieberegler“)",
            "Handschrift neben einem Graphen kommt mit „In den Graphen“ hinein",
            "Geteilt durch ist überall ein Bruch: in der Liste, in der Vorschau beim Tippen und in den Beschriftungen; Werte wie ⅓ stehen als Bruch statt 0,33",
            "Beim Tippen im Graph-Editor springt die Ansicht nicht mehr zum Graphen",
            "Eine gerade Linie halten wählt nur die Linie, auch direkt neben Schrift; an den Enden länger oder kürzer ziehen, der Winkel bleibt – deutlich zur Seite gezogen dreht sie sich",
            "PDFs mit n0te öffnen („Öffnen mit“): als neues Notizbuch oder in ein Notizbuch einfügen",
            "PDF-Export: Seitenblätter direkt nach ihrer Seite oder alle am Ende",
            "„An den Laptop“ schickt auch große Auswahlen ganz, nichts wird mehr abgeschnitten",
            "Große Auswahlen sind schnell: Lasso über eine volle Seite, Verschieben, Ablegen und Einfügen von tausenden Strichen",
        )),
        Entry("0.32.1", "24. Nov. 2026", "Updates von selbst", listOf(
            "Updates kommen jetzt von selbst aus n0tes GitHub-Repo (DiscoveryFox/n0te), ohne etwas einzustellen",
            "In den Einstellungen unter Updates lässt sich ein anderes Repo eintragen; ein leeres Feld schaltet Updates ab",
        )),
        Entry("0.32.0", "24. Nov. 2026", "Platz zur Seite", listOf(
            "Platz einfügen geht jetzt auch zur Seite: Stift setzen und nach rechts ziehen, alles rechts davon rückt weiter, die Seite wird breiter wenn nötig; nach links nimmt den Platz wieder heraus",
            "Wohin der Stift zuerst zieht, entscheidet die Richtung (ein kleines Kreuz zeigt die Stelle, bis es klar ist)",
            "Nach unten auch ohne etwas darunter: die Seite wird um den Platz länger, zum Weiterschreiben",
        )),
        Entry("0.31.2", "24. Nov. 2026", "Schönere Gliederung, besseres Daneben-Öffnen", listOf(
            "Gliederung: oben das Notizbuch mit Seiten und Gruppen, größere Seitenbilder, unter dem Namen die Seite und ihre Nebenseiten",
            "Gruppen mit Ordner und Anzahl, Linien zeigen, was in einer Gruppe liegt; die Seite in Sicht hat einen roten Rand",
            "Bilder-Ansicht: alle Bilder gleich hoch, die Nummer auf dem Bild",
            "⧉ Daneben öffnen: eine Seite dieses Notizbuchs aus ihren Bildern wählen, oder ein anderes Notizbuch mit Cover, Ordner und wann zuletzt offen; die Suche ist gleich da",
        )),
        Entry("0.31.1", "24. Nov. 2026", "Schieberegler von selbst, Karo in Groß", listOf(
            "Buchstaben, die es noch nicht gibt, werden zu Schiebereglern wie in GeoGebra: y = m x + c, f(x) = a(x − d)² + k, b = 4 + 3a",
            "„Groß“ zeigt die Kästchen des Karopapiers; der Graph rastet nach dem Verschieben und Zoomen an ihnen ein (1, 2, 5 … pro Kästchen) und sitzt danach genau auf den Linien der Seite",
            "Geteilter Bildschirm: langes Halten auf der rechten (unteren) Seite öffnet das Menü dort, nicht auf der anderen Seite",
            "„Groß“: die Leiste geht über die ganze Breite (wie GeoGebra Classic), so passt sie auch in eine Bildschirmhälfte und „Fertig“ ist immer da",
            "Mit angesteckter Tastatur liegt der Knopf „Tastatur am Bildschirm“ nicht mehr über der Farbleiste",
        )),
        Entry("0.31.0", "24. Nov. 2026", "Groß wie GeoGebra, Tastatur, Ordner", listOf(
            "Graph „Groß“: wie GeoGebra, Leiste links, Werkzeuge oben, der Graph füllt den ganzen Rest (nichts schwebt mehr)",
            "Im großen Graphen verschieben, zoomen und mit allen Werkzeugen arbeiten; zurück auf der Seite bleibt der Ausschnitt",
            "Die Eingabe zeigt beim Tippen schon die Formel (Brüche, Hochzahlen, Wurzeln)",
            "LaTeX geht auch: \\frac{1}{2}, \\sqrt{x}, \\pi … (die Tasten \\ und { } sind auf der Tastatur)",
            "Eine angesteckte Tastatur tippt in die Eingabe (Pfeile, Entf, Enter, Esc, Strg+A/C/V/X); die Bildschirmtastatur bleibt dann unten",
            "Neues Notizbuch: Ordner als Bilder zum Antippen, auch einen neuen gleich hier anlegen",
        )),
        Entry("0.30.0", "24. Nov. 2026", "Zwei Notizen nebeneinander", listOf(
            "⧉ in der Leiste öffnet ein zweites Notizbuch daneben, oder dasselbe nochmal (zwei Stellen zugleich)",
            "Eine Seite aus der Gliederung an den rechten Rand ziehen (oder „Daneben öffnen“) zeigt sie daneben",
            "Der Strich in der Mitte teilt den Platz (rastet bei einem Drittel, der Hälfte, zwei Dritteln); an den Rand gezogen schließt er",
            "Eine Leiste für den Bereich, der zuletzt berührt wurde (roter Rahmen); jeder Bereich hat oben Titel, Seite, ★, ⤢ und ✕",
            "★ Hauptbereich: Laptop (folgen, gefolgt werden), Laser und Herbeirufen gehören zu ihm; Striche gehen aus beiden live raus",
            "Dasselbe Notizbuch zweimal: was in einem Bereich geschrieben wird, steht sofort im anderen; ein gemeinsames Rückgängig",
            "Hochkant liegen die Bereiche übereinander; die Teilung bleibt, auch wenn Android die App zwischendurch beendet",
            "Seiten behalten beim Teilen und Drehen ihre Größe; eine Seite, die passte, passt weiter",
        )),
        Entry("0.29.1", "24. Nov. 2026", "Zuletzt bearbeitet", listOf(
            "Start zeigt die zuletzt bearbeiteten Seiten mit Bild, auch vom Laptop; ein Tipp öffnet genau diese Seite",
            "Das Lesen der Handschrift für die Suche zählt nicht als Bearbeiten",
            "Das ⋯ an einem Notizbuch öffnet nur noch sein Menü, nicht mehr das Notizbuch",
        )),
        Entry("0.29.0", "24. Nov. 2026", "Start, Gliederung mit Bildern", listOf(
            "Start in der Bibliothek: zuletzt geöffnet, angeheftet, die Ordner, was in keinem Ordner liegt",
            "Notizbücher als Kacheln oder als Liste; mit dem Finger halten und auf einen Ordner, einen Tag oder „Angeheftet“ ziehen",
            "Leere Ordner bleiben erhalten; Verschieben, Taggen und Anheften ändern die Reihenfolge unter „Zuletzt“ nicht mehr",
            "Gliederung mit Vorschaubildern der Seiten, als Liste oder als Seitenansicht; zeigt beim Öffnen die aktuelle Seite",
            "Seiten merken (Lesezeichen) und nur die gemerkten zeigen; Textseiten heißen nach ihrer ersten Zeile",
            "Unten in der Gliederung: neue Seite, neue Gruppe, alle Gruppen zu- oder aufklappen",
            "Die Vorschau eines Notizbuchs in der Bibliothek zeigt die Striche wieder an der richtigen Stelle",
        )),
        Entry("0.28.0", "24. Nov. 2026", "Ordnung in der Bibliothek", listOf(
            "Seitenleiste in der Bibliothek: alle Notizbücher, angeheftete, Ordner (auch ineinander, z. B. Uni › WS26), Tags",
            "Ordner anlegen, umbenennen, auflösen (Finger auf den Ordner halten); Notizbücher über „Verschieben“ hinein",
            "Tags mit eigener Farbe, an jedem Notizbuch; nach Tag filtern, Tags umbenennen oder überall entfernen",
            "Suche über Titel, Ordner und Tags; sortiert nach zuletzt geändert oder Name; Angeheftete stehen vorn",
            "„Auswählen“: mehrere Notizbücher auf einmal verschieben, taggen, anheften oder löschen",
            "Menüs leichter, mit weniger Text: das Menü am Notizbuch, und der Finger auf der Seite zeigt nur noch eine kleine Leiste",
            "Ein Dreieck als Pfeilspitze zeigt immer in die Richtung der Linie, egal wie es gezeichnet war; ein Zittern am Linienende kippt die Spitze nicht mehr",
        )),
        Entry("0.27.0", "23. Nov. 2026", "Updates aus GitHub", listOf(
            "n0te sucht nach neuen Versionen in den Releases eines GitHub-Repos: beim Öffnen, auf Wunsch auch alle 6 Stunden, oder von Hand (Einstellungen → Updates, dort das Repo eintragen)",
            "Ein neues Update wird im Hintergrund heruntergeladen und geprüft; dann fragt n0te: installieren oder später. Später: der Knopf oben auf dem Startbildschirm",
            "Schwung beim Verschieben: nie schneller als bei Android-Listen, auch wenn die App kurz beschäftigt war",
        )),
        Entry("0.26.1", "23. Nov. 2026", "PDF in jeder Seitengröße", listOf(
            "Der PDF-Export behält jede Seitengröße: eigene Formate, A3, durch „Platz einfügen“ verlängerte Seiten und PDF-Seiten, Boards so groß wie ihr Inhalt",
            "Boards im PDF: auch Bilder und Textfelder, die weiter weg liegen, kommen mit auf die Seite und an ihren Platz",
        )),
        Entry("0.26.0", "23. Nov. 2026", "Rechentastatur und großer Graph", listOf(
            "Eine eigene Rechentastatur wie in GeoGebra: Seiten 123, f(x), ABC und αβγ; Klammern kommen paarweise, Vorlagen wählen ihre Werte zum Überschreiben; die Systemtastatur bleibt weg",
            "„Groß“: beim Bearbeiten füllt der Graph den Platz rechts neben der Leiste (und rückt über die Tastatur); ausgeschaltet bleibt er wie auf der Seite und der Rest bleibt sichtbar. Nach „Fertig“ steht die Seite wieder wie vorher",
            "Die Algebra-Liste setzt Funktionen, Punkte und Gleichungen als Formeln wie GeoGebra: Brüche, Hochzahlen, Wurzeln, Vektoren als Spalten",
            "Die Leiste oben steht über dem Graphen, die Seiten links sind ein Umschalter, feine Linien trennen die Objekte",
            "x², x³, √ und π lassen sich direkt eintippen",
        )),
        Entry("0.25.0", "23. Nov. 2026", "Graphen einfacher bedienen", listOf(
            "Die Leiste links wie in der GeoGebra-App: Algebra, Werkzeuge (mit Namen, die wichtigsten zuerst, alle nach Gruppen auf Wunsch) und Grafik",
            "Oben nur noch Bewegen, die zuletzt benutzten Werkzeuge, Rückgängig und Fertig; Verwerfen fragt vorher nach",
            "Beim Konstruieren sagt die Leiste unten Schritt für Schritt, was als Nächstes anzutippen ist",
            "Ein Objekt antippen: es wird im Graphen hervorgehoben, unten gibt es Farbe, Linie, Beschriftung, Einstellungen und Löschen",
            "Die Algebra-Liste zeigt die Definition nur, wo sie etwas sagt (Schneide(f, c, 1), f'(x)); Löschen nur am ausgewählten Objekt",
            "Beim Tippen schlägt die Eingabezeile passende Befehle vor; die Tasten für x, ², √ … erscheinen nur beim Tippen; ein leerer Graph bietet Beispiele an",
            "Gelöscht: die Meldung unten bietet Rückgängig an",
            "Neue Werkzeuge Nullstellen und Extrempunkte; Ansicht zurücksetzen",
            "Beschriftungen von Punkten weichen auch den Zahlen an den Achsen aus",
            "Einen ausgewählten Graphen an den Laptop senden; Graphen und Bilder sind im Bild für den Laptop mit dabei",
        )),
        Entry("0.24.0", "23. Nov. 2026", "Graphen wie in GeoGebra", listOf(
            "Ein Graph ist jetzt eine Konstruktion wie in GeoGebra: Punkte, Geraden, Strecken, Strahlen, Vektoren, Kreise, Vielecke, Winkel, Kegelschnitte und Gleichungen, Texte, Schieberegler und Funktionen, auch mit Wenn(…)",
            "Werkzeugleiste mit den GeoGebra-Werkzeugen und -Bildern: Bewegen, Punkt, Schneide, Mittelpunkt, Gerade, Strecke, Senkrechte, Parallele, Tangente, Vieleck, Kreis, Winkel, Abstand, Spiegeln, Drehen, Schieberegler, Text und mehr",
            "Algebra-Ansicht links: jedes Objekt mit Wert und Definition, antippen zum Ändern, Punkt zum Ein- und Ausblenden. Die Eingabezeile versteht die GeoGebra-Befehle auf Deutsch und Englisch (Schneide, Kreis, Nullstelle, Ableitung, Integral …)",
            "Eigenschaften je Objekt: Farbe, Linie, Punktform, Füllung, Beschriftung (Name, Name und Wert, Wert, eigener Text), fixiert, Hilfsobjekt",
            "Punktfang automatisch, am Raster oder aus; auf Karopapier rasten Punkte an den Kästchen ein",
            "Interaktiv (je Graph einschaltbar): Punkte und Schieberegler direkt auf der Seite ziehen, alles Abhängige geht mit",
            "Zwei Schnittpunkte werden zwei Punkte (A und B), jeder für sich weiter nutzbar; umbenennen nimmt die Bezüge mit",
            "Alles bleibt Tinte: auf dem Laptop und im PDF sieht der Graph genauso aus. Der Laptop hat dieselbe Algebra-Ansicht mit Eingabezeile",
        )),
        Entry("0.23.1", "23. Nov. 2026", "Graphen genauer", listOf(
            "Nullstellen und Schnittpunkte genau auf runden Stellen (sin x bei π, x = 0,3) werden gefunden",
            "Beschriftungen an Punkten und Kurven weichen den Kurven und Achsen aus",
            "Eine Funktion entfernen: die darunter rücken einen Namen auf, Bezüge wie h(x) wandern mit",
        )),
        Entry("0.23.0", "23. Nov. 2026", "Graphen, mehr davon", listOf(
            "Lange mit dem Finger auf etwas halten markiert es: ein Bild, einen Graphen, ein geschriebenes Wort, eine Linie, und der Finger kann es gleich verschieben. Auf einer leeren Stelle kommt wie bisher das Menü",
            "Neuer Graph: Plus → Graph, dann ein Rechteck aufziehen (rastet auf Karopapier an den Kästchen ein)",
            "Bearbeiten in einer Leiste neben der Seite: was du tippst, steht sofort im Graphen. Vorlagen für leere Funktionen",
            "Funktionen beziehen sich aufeinander: f'(x), g(x) = f(x) + 1, x = 2 ist eine senkrechte Linie; zu jeder Funktion „Ableitung dazu“",
            "Punkte: Nullstellen, Hoch- und Tiefpunkte, Schnitt- und Wendepunkte werden markiert, auf Wunsch mit Koordinaten; eigene Punkte wie A(1|2)",
            "Flächen schraffieren: unter einer Funktion oder zwischen zweien, von a bis b, mit ihrem Wert",
            "Gestrichelte und gepunktete Linien, Namen an den Kurven, x-Achse in π-Schritten, eigene Achsennamen",
            "Handschrift als Graph: f(x) = … schreiben, mit dem Lasso auswählen → Graph (gelesen wird auf dem Laptop)",
            "Die Suche findet auch Funktionen in Graphen",
        )),
        Entry("0.22.0", "23. Nov. 2026", "Graphen", listOf(
            "Koordinatensysteme mit Funktionsgraphen: Plus → Graph, oder den Finger auf die Stelle halten → Koordinatensystem hier",
            "Funktionen einfach eintippen: x^2 - 1, 2sin x, |x|, √x, e^-x, 0,5x … mit Tasten für das, was die Tastatur nicht hat, und einer Vorschau",
            "Auf Karopapier rastet es an den Kästchen ein, ein Kästchen ist eine Einheit",
            "Sieht gezeichnet aus, nicht gedruckt: Achsen, Zahlen und Kurven wie mit dem Stift; „Wie mit dem Lineal“ macht es gerade",
            "Bleibt bearbeitbar: mit dem Lasso antippen → Bearbeiten (Funktionen, Bereich, „y passend“, Raster); verschieben und größer ziehen wie ein Bild",
            "Polstellen und Sprünge (1/x, tan x, floor) werden nicht verbunden",
        )),
        Entry("0.21.0", "21. Nov. 2026", "Handschrift finden", listOf(
            "Suchen in der Gliederung: findet Handschrift, getippten Text, Überschriften und den Text von PDFs, auch mit kleinen Lesefehlern und ohne auf Groß- und Kleinschreibung oder Umlaute zu achten",
            "Ein Treffer antippen: die Seite springt dorthin und die Stelle leuchtet kurz auf",
            "Die Handschrift liest das Tablet selbst (Google ML Kit, offline); die Erkennung wird einmal über WLAN geladen, etwa 20 MB. Ausschaltbar unter Stift & Eingabe → Handschrift durchsuchbar",
            "Überschriften: Geschriebenes mit dem Lasso auswählen → Überschrift. Sie steht dann eingerückt unter ihrer Seite in der Gliederung und führt per Antippen dorthin; halten nimmt sie wieder weg",
            "Eine Seite ohne eigenen Namen heißt in der Gliederung wie ihre erste Überschrift",
        )),
        Entry("0.20.4", "21. Nov. 2026", "Spitze vorn", listOf(
            "Die Linie schaut nicht mehr vorn durch die Pfeilspitze: die Spitze bleibt beim Zeichnen ganz spitz und deckt das Linienende ab",
        )),
        Entry("0.20.3", "21. Nov. 2026", "Spitzen in Linie", listOf(
            "Die saubere Pfeilspitze liegt jetzt genau symmetrisch zur Linie, auch wenn die gezeichnete schief saß",
            "Eine sehr groß gezeichnete Spitze wird kleiner: höchstens ein knappes Drittel der Linie",
            "Pfeile brauchen ein längeres Halten (knapp eine Sekunde) als Formen: kurze Denkpausen beim Schreiben machen keine Spitzen mehr",
            "Winzige Striche an kurzen Linien (Buchstaben) werden nie zu Spitzen",
        )),
        Entry("0.20.2", "21. Nov. 2026", "Pfeile nur auf Wunsch", listOf(
            "Pfeilspitzen entstehen nur noch, wenn du am Ende der Spitze kurz still hältst. Beim Schreiben wird nichts mehr zur Pfeilspitze (etwa ein n)",
            "Pfeile lassen sich ausschalten: Einstellungen → Pfeile erkennen",
            "Spitzen an Kurven, die sich am Ende einrollen oder mit einem kleinen Schwung enden, zeigen jetzt in die richtige Richtung",
            "Die Spitze darf auch von ihrer Spitze aus gezeichnet sein (hin, zurück, zum zweiten Strich) oder als zwei einzelne Striche",
            "Ein einzelnes Zeichen auswählen: von innen verschieben, die Größe an den Ecken und Seiten des Rahmens ändern. Griffe an den Strichenden gibt es nur noch bei geraden Linien",
        )),
        Entry("0.20.1", "21. Nov. 2026", "Pfeile, die klappen", listOf(
            "Pfeilspitzen passen jetzt auch an gebogene Linien. Das V darf einen kleinen Haken haben, etwas neben dem Linienende ansetzen oder ein kleines geschlossenes Dreieck sein",
            "Ein Pfeil in einem Zug (Linie, dann beide Spitzenstriche) wird auch ohne Halten aufgeräumt. Gebogene Pfeile bleiben so gebogen, wie du sie gezeichnet hast",
            "Striche dürfen außerhalb der Seite beginnen: die Linie fängt genau am Seitenrand an. Der Radierer genauso",
            "Lasso: eine lange Linie ist gewählt, sobald etwa die Hälfte ihrer Länge im Lasso liegt. Oder sie einfach mit dem Auswahlwerkzeug antippen",
            "Werkzeugleiste: keine dunklen Ringe mehr um Werkzeuge, über denen der Stift geschwebt hat",
        )),
        Entry("0.20.0", "21. Nov. 2026", "Platz, Pfeile, Griffe", listOf(
            "Platz einfügen: am Auswahl-Knopf (viertes Werkzeug) oder Finger halten → Platz einfügen. Stift auf die Stelle, nach unten ziehen: alles darunter rückt nach, auf liniertem und kariertem Papier in ganzen Zeilen. Nach oben ziehen nimmt leeren Platz wieder weg",
            "Reicht der Platz unten nicht, wird die Seite länger: nichts rutscht auf die nächste Seite",
            "Auf PDF-Seiten teilt sich das PDF an der Linie: was darüber steht, bleibt; darunter rückt das Blatt mit deinen Notizen nach unten. Auch auf dem Laptop und im PDF-Export",
            "Pfeile: eine Linie ziehen, dann ein V an ihr Ende – es wird eine saubere Spitze. Oder den Pfeil in einem Zug zeichnen und am Ende halten. Er bleibt handgezeichnet: dein Druck und ein leichter Schwung bleiben",
            "Auswahl: eine Form mit dem Auswahlwerkzeug antippen. Griffe an den Seiten ändern nur Breite oder Höhe (die Linie bleibt gleich dick), Ecken skalieren gleichmäßig, der Knopf oben dreht und rastet alle 15° ein",
            "Linien und Formen beginnen wieder genau dort, wo der Stift aufsetzt",
            "Eine Seite, die ihre Größe ändert, zeigt ihre Striche sofort ganz",
        )),
        Entry("0.19.0", "21. Nov. 2026", "Formen nach Maß", listOf(
            "Form zeichnen, halten bis sie einrastet, dann ohne abzusetzen weiterziehen: die Ecke beim Stift folgt, die gegenüberliegende bleibt stehen, so ändern sich Breite und Höhe (nur seitwärts ziehen ändert nur die Breite)",
            "Rechtecke und Ellipsen behalten dabei ihre Richtung und werden mit einem Ticken zum Quadrat oder Kreis, wenn die Seiten fast gleich lang sind",
            "Kreise und fast waagrechte Ellipsen liegen jetzt genau gerade",
        )),
        Entry("0.18.1", "21. Nov. 2026", "Ein Viereck ist ein Viereck", listOf(
            "Formerkennung: das Zittern des Stifts beim Aufsetzen und beim Halten am Ende wird nicht mehr als Ecke gelesen (daraus wurden schiefe Vierecke, oder es wurde gar nichts erkannt)",
            "Erkannte Formen haben an allen Ecken scharfe Spitzen (bisher war nur die Ecke am Anfang spitz, die anderen abgeschliffen)",
            "Rechtecke werden gerade: die Richtung kommt aus allen vier Seiten, bis 10° schief wird waagrecht; fast gleich lange Seiten ergeben ein Quadrat",
            "Sehr runde Ecken verbiegen die Seiten nicht mehr: jede Seite wird aus ihrer geraden Mitte gelegt",
            "Eine Ecke gleich am Anfang des Strichs geht nicht mehr verloren, und eine abgerundete Ecke wird kein Fünf- oder Sechseck",
        )),
        Entry("0.18.0", "20. Nov. 2026", "Formen, die passen", listOf(
            "Formerkennung nochmal neu: Rechtecke, Quadrate, Dreiecke und Kreise werden auch erkannt, wenn das Ende nicht ganz zum Anfang findet, eine Ecke sehr rund ist oder der Strich am Anfang hakt (im Test 99 bis 100 % von je 200 handgezeichneten Formen)",
            "Ein gerundetes Quadrat wird kein Kreis mehr, ein wackliger Kreis kein Vieleck",
            "Gehaltene Linie: kurz quer hin und her wackeln schaltet das Einrasten aus, nochmal wackeln wieder an",
            "Schreiblupe: auch mit dem Finger weiter, durch Antippen im Bereich rechts (am Zeilenende in die nächste Zeile)",
            "Mitwandern: auch ein Finger-Tippen im Streifen rechts schiebt weiter",
        )),
        Entry("0.17.0", "20. Nov. 2026", "Gerade Linien, saubere Formen", listOf(
            "Linie zeichnen, den Stift halten, dann weiterziehen: Länge und Richtung ändern sich, die Linie bleibt gerade und rastet bei 15° und 45° ein (mit einem kurzen Ticken)",
            "Formerkennung neu: lockere Rechtecke, Dreiecke und Vielecke werden sauber, auch mit runden Ecken oder einem Schnörkel am Ende; fast waagrechte und senkrechte Seiten werden genau gerade",
            "Schreiblupe: Antippen im Bereich rechts rückt sofort weiter",
            "Schreiblupe: Leisten unten und links zeigen, wo der Rahmen auf der Seite ist; mit dem Finger ziehen bringt ihn überall hin",
            "Mitwandern: Antippen im Streifen rechts schiebt sofort weiter",
        )),
        Entry("0.16.0", "20. Nov. 2026", "Die Schreiblupe, aufgeräumt", listOf(
            "Das Schreibfeld lässt sich am Griff oben verschieben, auch in die Mitte oder nach oben, und am Knopf rechts in der Höhe ändern; es merkt sich die Stelle",
            "Der Rahmen auf der Seite: mit dem Finger verschieben, an der Ecke größer oder kleiner ziehen (oder zwei Finger im Schreibfeld)",
            "Auf Linien, Kästchen und Punkten sitzt der Rahmen auf der Linie; neue Zeilen folgen dem Papier, ein Strich zeigt, wo Zeilen beginnen",
            "Knöpfe am Schreibfeld: zurück, weiter, neue Zeile, schließen. Der Bereich rechts wird kräftiger, sobald du über die Mitte schreibst",
            "Schreiblupe auch über das Finger-Halten-Menü: „Schreiblupe hier“",
            "Neues Symbol in der Leiste: ein Stift über einem Schreibfeld",
        )),
        Entry("0.15.0", "20. Nov. 2026", "Schreiben im Fluss", listOf(
            "Schreiblupe (Lupe oben in der Leiste): unten groß schreiben, es landet klein im Rahmen auf der Seite. Die Seite bewegt sich nicht unter dem Stift",
            "In der Lupe: Stift im schraffierten Teil rechts absetzen rückt weiter, am Seitenrand in die nächste Zeile; Finger im Streifen verschiebt den Rahmen, Stift-Tippen auf die Seite setzt ihn",
            "Mitwandern neu: ein markierter Streifen rechts zeigt, wo es wirkt. Nur den Stift darin absetzen schiebt weiter, eine Pause allein nie",
            "Am Seitenrand eine Zeile tiefer (Linien: eine Linie, Kästchen: ganze Kästchen) und zurück an den linken Rand",
            "Auswahl über mehrere Seiten auf einmal, und sie darf neben der Seite beginnen",
            "Einfügen bleibt auf der Seite, wenn es draufpasst",
        )),
        Entry("0.14.0", "20. Nov. 2026", "Seiten in jedem Format", listOf(
            "Seiten drehen: unten rechts auf die Seitenzahl tippen (oder den Finger auf die Seite halten) → Format → Hoch oder Quer",
            "Andere Formate: A3, A4, A5, A6, Letter, Quadrat, Folie 16:9, oder eine eigene Größe in Millimetern",
            "Ragt beim Drehen oder Verkleinern etwas über den Rand, fragt n0te vorher nach; gelöscht wird nie etwas, zurückdrehen holt alles wieder",
            "„Für alle Seiten übernehmen“ gibt Papier und Format an jede Seite weiter; neue Seiten übernehmen das Format der aktuellen",
            "Format für neue Notizbücher in den Einstellungen unter Papier",
            "Aufgeräumt: ruhigere Menüs in der Schrift des Designs, schlanke Regler, Abstände und Strichstärken in Millimetern",
            "Die Plus-Knöpfe an der Seite sind dezenter, Menüs sind nicht mehr durchscheinend",
        )),
        Entry("0.13.0", "20. Nov. 2026", "Zusammen hinschauen", listOf(
            "Gäste können zum Besitzer springen oder ihm folgen (Chip unten rechts); selbst verschieben beendet das Folgen",
            "Gäste zu dir holen: einen oder alle, sie sehen sofort, was du siehst",
            "Tippen auf einen Gast im Chip-Menü zeigt, wo er gerade ist",
            "Ruhigere Gäste-Oberfläche: Alltägliches im Chip-Menü, Einladungen als kurze Zeilen, Einstellungen aufklappbar",
            "Neuer Bereich Diagnose in den Einstellungen: Stift-Test mit Linie, Druck, Neigung, Abtastrate",
        )),
        Entry("0.12.0", "20. Nov. 2026", "Einladungen im Griff", listOf(
            "Einen einzelnen Gast entfernen (Gäste-Übersicht → Entfernen): er kommt mit dem Link nicht mehr herein, alle anderen bleiben; „Wieder zulassen“ macht es rückgängig",
            "Beitritt bestätigen: neue Gäste warten, bis du sie hereinlässt (der Chip unten rechts zeigt, wer wartet)",
            "Nur ein Gerät: der Link gilt nur für das erste Gerät",
            "PIN: neue Gäste müssen eine Zahl eingeben, die du ihnen getrennt vom Link sagst",
            "Ablauf ändern: 1 Tag, 7 Tage, 30 Tage oder ohne Ablauf, auch für schon verschickte Links",
            "Wer einmal drin ist, braucht weder PIN noch Bestätigung nochmal",
            "Kopieren, Ausschneiden und „Laptop“ schließen die Auswahl-Leiste",
            "Finger auf die Seite halten: Menü mit Einfügen, Bild einfügen und Text hier, genau an dieser Stelle",
        )),
        Entry("0.11.0", "19. Nov. 2026", "Gäste, die wirklich mitmachen", listOf(
            "Gäste-Übersicht im Notizbuch: wer gerade da ist, auf welcher Seite, mit welchen Rechten",
            "Rechte jederzeit umstellen: nur lesen ↔ lesen & schreiben, der Gast bekommt es sofort",
            "Was Gäste schreiben, siehst du jetzt live (vorher nur umgekehrt)",
            "Wo Gäste hinschauen, als Rahmen mit Namen; abschaltbar",
            "Ob Gäste sehen, wo du hinschaust, kannst du ein- und ausschalten",
            "Beitreten wartet, bis das Notizbuch da ist: kein Fehler mehr beim ersten Öffnen",
            "Nur-lesen-Gäste können nichts mehr verändern; ein Hinweis zeigt es",
            "Eigener Name für Laptop und Gäste (Einstellungen → Laptop)",
            "Einladungslinks öffnen n0te direkt, ein kopierter Link wird beim Beitreten eingesetzt",
            "Diese Neuigkeiten: einmal nach jedem Update, immer in den Einstellungen",
        )),
        Entry("0.10.1", "19. Nov. 2026", "Bilder aus anderen Apps", listOf(
            "Bilder per Drag & Drop hineinziehen (auch aus der Samsung-Leiste und geteiltem Bildschirm)",
            "Bilder und PDFs aus anderen Apps mit „Teilen“ an n0te schicken",
            "Bilder aus der Zwischenablage einfügen",
            "Kompaktere Menüs für + und den Stift",
        )),
        Entry("0.10.0", "19. Nov. 2026", "Bilder und Ausschnitte", listOf(
            "Bilder auf Seiten einfügen, verschieben und skalieren",
            "Ausschnitt einer Seite als Bild: in voller Auflösung, auch bei PDF-Hintergrund",
            "Rechteck-Auswahl neben der freien Auswahl",
        )),
        Entry("0.9.0", "19. Nov. 2026", "Aus deiner Ideenliste", listOf(
            "Füller mit Optionen: Empfindlichkeit, Tempo, Federform, auslaufende Enden",
            "Schreibfluss: die Seite rückt beim Schreiben mit",
            "Linienenden nachträglich verschieben",
            "Radierer: über den Knopf wischen wechselt die Art",
            "Laserpointer mit Spur",
        )),
        Entry("0.8.3", "19. Nov. 2026", "Weniger Versehen", listOf(
            "Ein Notizbuch öffnet dort, wo du aufgehört hast",
            "Neue Seiten erst nach Halten auf + (Ring füllt sich; Dauer einstellbar oder sofort)",
            "Seiten löschen, mit Rückgängig",
            "Stift, Farbe und Breite bleiben gemerkt",
        )),
        Entry("0.8.1", "19. Nov. 2026", "PDF-Korrektur", listOf(
            "Importierte PDFs verlieren beim Zoomen keine Streifen mehr",
        )),
        Entry("0.8.0", "19. Nov. 2026", "Designs mit eigenem Charakter", listOf(
            "Jedes Design hat eigene Formen, Knöpfe und Schriften (Nothing, Hell, OLED, Ozean, Wald, Sepia)",
            "Aufgeräumte Bibliothek, Einstellungen und Export",
            "Beim Erstellen Papier, Kästchengröße und Format wählen; neue Seiten behalten sie",
        )),
        Entry("0.7.8", "19. Nov. 2026", "Koppeln per QR-Code", listOf(
            "Tablet und Laptop per QR-Code oder Code koppeln, ohne n0bridge",
            "Der Sync-Start versucht es von selbst erneut",
        )),
        Entry("0.7.7", "19. Nov. 2026", "Formeln und Formen", listOf(
            "Handschrift zu LaTeX, zentrierte Formeln",
            "Formen rasten ein",
            "Standard-Seitengröße einstellbar",
        )),
        Entry("0.7.5", "19. Nov. 2026", "Tippen repariert", listOf(
            "Beim Tippen ging nicht mehr jedes zweite Zeichen verloren",
            "Radieren zeigt sich live auf dem anderen Gerät; der rote Ansichtsrahmen ist abschaltbar",
        )),
        Entry("0.7.0", "19. Nov. 2026", "Schreiben wie im Textprogramm", listOf(
            "Textseiten und Textfelder auf dem Tablet bearbeiten (fett, kursiv, Überschriften)",
            "LaTeX-Formeln im Text",
            "Live-Steuerung und Verbindungsanzeige",
        )),
    )

    private fun version(ctx: Context) = runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull().orEmpty()

    /** The entry to show once: this version's, unless already seen (after install and updates). */
    fun unseen(ctx: Context): Entry? {
        val v = version(ctx)
        if (v.isEmpty() || Settings.str("changelog_seen", "") == v) return null
        return all.firstOrNull { it.version == v } ?: all.first()
    }

    fun markSeen(ctx: Context) = Settings.set("changelog_seen", version(ctx))
}
