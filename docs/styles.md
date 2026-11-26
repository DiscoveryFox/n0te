# Designs: write your own

n0te's look is a **design**: colours, corner shapes, the voice of labels and titles, and optional
decorations behind the library (snow, fairy lights, stars …). n0te ships with fourteen designs.
You can make your own as a small JSON file, usually with the help of an AI chat. A file imports
the same way on the Android tablet and on n0te Laptop / n0te Tablet for Windows and Linux.

## The quick way: ask an AI

1. **Tablet:** Settings › Design › *Mit KI gestalten* › *Anleitung kopieren* (or *An KI-App senden*).
   **Laptop:** Settings › Design › *KI-Anleitung kopieren*.
2. Paste the instructions into an AI chat (ChatGPT, Claude, Gemini, …). Replace the line
   "My wish" ("Mein Wunsch") with your idea, for example *"calm like a Japanese tea house"* or
   *"my school colours, dark blue and gold, a bit playful"*.
3. Copy the AI's whole answer. You don't need to cut out the JSON: n0te finds it inside the text and
   ignores code fences, comments and trailing commas.
4. **Tablet:** *Antwort einfügen* (or *Aus Zwischenablage*). **Laptop:** *Antwort einfügen*. You can
   also save the answer as a `.json` file, then open it (tablet: *Datei öffnen*) or drop it on
   the laptop window.
5. n0te checks the design and shows a preview: *Übernehmen* (apply) or *Nur speichern* (just
   save). If something is wrong, n0te lists every problem. *Fehler kopieren* puts the list on the
   clipboard with a request to fix it. Paste it back to the AI and import its new answer.

To share a design, use the copy/share button on its card. To move it from the tablet to the
laptop or back, copy it on one device and paste it on the other.

## The format (version 1)

```json
{
  "n0te_style": 1,
  "name": "Weihnachten",
  "name_en": "Christmas",
  "description": "Tannengrün und Beerenrot, Schneefall und Lichterkette",
  "description_en": "Pine green and berry red, snowfall and fairy lights",
  "author": "you",
  "dark": true,
  "colors": {
    "background": "#0e2419",
    "surface": "#15301f",
    "surface_2": "#1d3b28",
    "border": "#2c4c37",
    "text": "#f6efe1",
    "muted": "#9db3a2",
    "accent": "#d7263d",
    "on_accent": "#ffffff",
    "paper": "#fffdf7"
  },
  "corners": { "buttons": 14, "cards": 18, "panels": 22 },
  "labels": "sans",
  "headings": "serif",
  "fill": "tonal",
  "shadows": true,
  "decorations": [
    { "kind": "snow", "colors": ["#ffffff"], "amount": 0.55 },
    { "kind": "lights", "colors": ["#f5c542", "#d7263d", "#3fa34d", "#4a90e2"], "amount": 0.7 }
  ]
}
```

| Key | Required | Meaning |
|---|:---:|---|
| `n0te_style` | – | Format version. Always `1` for now; a newer number asks you to update n0te. |
| `name` | ● | The name on the card, up to 40 characters. `name_en` is the English name, shown when n0te runs in English. |
| `description`, `description_en` | – | One line about the look, up to 120 characters. |
| `author` | – | Shown as "by …" on the card. |
| `dark` | – | `true` for dark designs. If it is missing, n0te works it out from the background. |
| `colors.background` | ● | The window behind everything. |
| `colors.surface` | ● | Cards, bars, menus, dialogs. |
| `colors.text` | ● | Main text. It needs a contrast of at least 4.5:1 with `background` and with `surface`. Below 3:1 n0te refuses the file. |
| `colors.accent` | ● | The one signal colour: the main button, selections, sliders. |
| `colors.surface_2` | – | One step above `surface`: pressed buttons, fields. |
| `colors.border` | – | Hairlines and outlines. |
| `colors.muted` | – | Secondary text: hints and counts. |
| `colors.on_accent` | – | Text on the accent. If it is missing, n0te picks white or near-black, whichever reads better. |
| `colors.paper` | – | The page colour on screen (exported PDFs stay white). It **must be light**, because ink is dark. Defaults to white. |
| `corners` | – | Radii from 0 (square) to 32, as one number or `{ "buttons", "cards", "panels" }`. From about 20, buttons become pills. |
| `labels` | – | The voice of button labels: `"mono_caps"` (technical capitals), `"sans"` (modern), `"serif"` (bookish). |
| `headings` | – | The title face: `"dot"` (dot matrix), `"mono"`, `"sans"`, `"serif"`. |
| `fill` | – | How secondary buttons and cards are drawn: `"outline"` (thin frames), `"tonal"` (tinted surfaces without frames), `"rule"` (hairlines). |
| `shadows` | – | Soft shadows under cards and dialogs. |
| `decorations` | – | Up to three, drawn behind the notebook library: `snow`, `lights`, `stars`, `grid`, `petals`, `leaves`, `scanlines`. Each takes `colors` (a list) and `amount` (0 to 1). You can also write just the name: `"decorations": ["snow"]`. |

Colours are written `"#RRGGBB"`, `"#RGB"` or `"rgb(r, g, b)"`.

**Errors and warnings.** Errors stop the import: missing required keys, values that aren't
colours, unknown labels, headings or fills, unreadable text, dark paper, a file over 64 KB. n0te
lists them all at once.

Warnings don't stop the import; n0te shows what it adjusted:

- unknown keys, with a suggestion when it looks like a typo ("acent" → "accent");
- radii outside 0 to 32;
- low but still readable contrast;
- unknown decorations;
- more than three decorations.

**Storage.**

- **Tablet:** imported designs live in the app's own storage.
- **Linux laptop:** `~/.config/n0te/designs/`.
- **Windows laptop:** `%APPDATA%\n0te\config\designs\`.

There is one `<id>.json` per design. If you import a design with the same name again, it replaces
the old one.

Decorations move only behind the library, never while you write. They can be switched off, or
held still, under Settings › Design. They also hold still when Android's "remove animations" is
on.

## The built-in designs

**Classic:**

- Nothing
- Hell (Light)
- OLED
- Ozean (Ocean)
- Wald (Forest)
- Sepia

**Special:**

- Weihnachten (Christmas): snow and fairy lights.
- Blaupause (Blueprint): a drafting grid.
- Kreidetafel (Chalkboard).
- Sternennacht (Starry night): twinkling stars.
- Kirschblüte (Cherry blossom): falling petals.
- Herbst (Autumn): falling leaves.
- Terminal: scanlines.
- Hoher Kontrast (High contrast).

Their files are in [`crates/core/designs/`](../crates/core/designs/) and make good starting
points.

## Ideas for designs

Ideas for your own designs, or for later built-in ones:

- **Seasons and holidays:**
  - Halloween (black and pumpkin orange, bats);
  - Easter pastels;
  - New Year (midnight blue, gold and confetti);
  - Summer (sand, sea glass, sun).
- **Places:**
  - Japanese tea house (rice paper, matcha, ink black);
  - library at night (leather brown, brass, lamp light);
  - Nordic cabin (birch, snow grey, red wool);
  - harbour (navy and signal yellow);
  - desert dusk (terracotta, dune, violet sky).
- **Materials:**
  - kraft paper;
  - linen;
  - slate;
  - copper;
  - concrete;
  - graph paper.
- **Eras and media:**
  - 70s (mustard, avocado, rounded);
  - 80s neon (magenta and cyan);
  - newspaper (grey on newsprint, serif, rules);
  - Game Boy (four greens, square);
  - typewriter (cream, mono, ribbon red).
- **For focus and accessibility:**
  - a calm low-blue night design;
  - colour-blind-safe accents (blue and orange instead of red and green);
  - large-contrast amber on black.
- **School and university:** your school's or faculty's colours. The AI prompt handles "my
  school colours are …".
