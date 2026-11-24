# WordCraft — EMEI/UHB edition (Windows)

Portable build: nothing is installed and no administrator rights are needed.

## Start

1. Unzip the whole folder somewhere you can write to (e.g. your Documents or H: drive).
   Keep `WordCraft.exe` and the `EMEI Templates` folder together.
2. Run `WordCraft.exe`. Windows may warn that the program is unrecognised (it isn't code-signed):
   choose **More info › Run anyway**, or ask IT to allow it.
3. **File › New** lists the templates under **EMEI**, **UHB** and **Job Instructions**.
   (If you have set your own personal templates folder in File › Options › Advanced › File
   locations, that one is used instead; clear it to get these back.)

## Writing

The **EMEI** tab (after Home) has the house-style buttons:

- **Mode**: *Insert* adds a new paragraph after the current one; *Restyle* changes the selected
  paragraphs instead.
- **Headings**, **Numbered**, **No Number**: the EMEI/UHB paragraph levels. A level more than one
  deeper than the paragraph before it is refused.
- **Inserts**: Warning, Caution, Note, Graphic (title, then choose the picture), Table (title, size,
  position, widths), In-line Heading.
- **Document**: Contents, List of Figures, List of Tables, Update All, Insert Chapter (UHB), Draft.
- **Level**: Promote, Demote.

Press **Update All** before saving a finished document, so numbers, contents and lists are current.

## Saving for the Defence network

Save as **Word Document (.docx)**. The file holds text, styles, pictures and fields only: no
macros or code. Open it once in Word to confirm it looks right before uploading it to Objective.

## Command line

`wordcraft-cli.exe` runs the same actions from scripts, e.g.
`wordcraft-cli commands` lists them (the EMEI ones start with `emei.`).
