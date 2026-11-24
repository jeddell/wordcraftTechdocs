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
- Known: UHB captions use the legacy chapter scheme (`Chapter {SEQ Chapter}`,
  `Table {SEQ Chapter}-{SEQ Table}`). They display correctly but WordCraft's lists of
  figures/tables do not read them yet; to be resolved with Insert Chapter (step 5).
- Steps 3–6: EMEI ribbon tab, inserts, document tools, packaging. Not started.
