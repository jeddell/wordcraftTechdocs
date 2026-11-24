# EMEI / UHB technical publications kit

Fork-only additions for writing Australian Defence EMEIs and UHBs in WordCraft. Everything
here lives in its own folder so syncing this fork with storytold/wordcraft stays conflict-free;
the only edits to WordCraft's own code are small, marked hooks.

Documents produced are plain `.docx`: styles, numbering, captions and pictures only, no macros.

## Layout

| Path | What |
|---|---|
| `templates/EMEI/` | EMEI instruction templates (`.dotx`) |
| `templates/UHB/` | UHB section templates and master templates (`.dotx`) |
| `templates/Job Instructions/` | Job Instruction masters (`.dotx`, EMEI style set) |
| `assets/wcn/` | Warning and Caution symbols used by the Warning/Caution/Note button |
| `tools/convert_templates.py` | Rebuilds `templates/` from the legacy `.dot` files |

Point WordCraft's personal templates folder (File › Options › Advanced › File locations) at
`emei/templates`: File › New then lists EMEI, UHB and Job Instructions as categories.

## Rebuilding the templates

```sh
python3 emei/tools/convert_templates.py "<old add-in>/Templates"
```

Needs LibreOffice (`soffice`) on the PATH. The script converts each legacy template, removes
the old label numbering on the Figure/Table title styles (captions now read "Table 1"), bases
those styles on Caption so lists of figures/tables find them, restores Times New Roman/Arial and
Australian English where the conversion substituted them, removes the link to the old add-in,
and tags each template with the document property `EMEIUHB = EMEI | UHB`.

## Status

- Step 1: fork builds; CLI opens and renders all templates. Done.
- Step 2: 44 templates converted and verified (no macro parts, open and render in WordCraft,
  EMEI list of tables fills). Done.
- Step 3: EMEI tab (after Home): Insert/Restyle toggle; Division/Main, Group and Para headings;
  Paragraph and SubPara 1–4, numbered and unnumbered (table variants inside tables); Promote and
  Demote; the legacy one-level-deeper rule. Done.
- Step 4: Inserts: Warning, Caution, Note (symbols compiled in from `assets/wcn/`); Graphic (title
  dialog, then the picture chooser; caption "Figure n    Title" below, UHB "Figure c-n"); Table
  (title, rows, columns, position/indent, row height, column widths; title above, grid borders);
  In-line Heading. Done.
- Step 5: Document group: Contents (EMEI Division/Group/Para headings; UHB Section/Chapter/Main/
  Group), List of Figures, List of Tables, Update All, Insert Chapter (UHB: new page,
  `Chapter {SEQ Chapter}` resetting figure/table numbers, title, Main heading; optional starting
  number per file), Draft (diagonal DRAFT watermark). UHB captions `Figure {SEQ Chapter \c}-{SEQ
  Figure}` are listed: WordCraft's list builder now matches a caption by any of its SEQ fields, as
  Word does (`crates/engine/src/cmd/citations/generated.rs`, marked "fork"). Done.
- Step 6: packaging.

Tip: add the contents and lists when the body is written (or in their own file, as UHB AI/AJ/AK
are), then press Update All after later edits.

All tab actions are engine commands (`wordcraft-cli commands | grep ^emei.`), so they also work
from scripts: see `crates/engine/src/cmd/emei.rs` for parameters.
