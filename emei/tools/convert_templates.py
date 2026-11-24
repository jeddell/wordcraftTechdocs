#!/usr/bin/env python3
"""Convert the legacy EMEI/UHB Word 97-2003 templates (.dot) into clean .dotx templates.

Usage:
    python3 emei/tools/convert_templates.py SOURCE_DIR [OUT_DIR]

SOURCE_DIR holds the legacy `.dot` files (the old add-in's `Templates` folder).
OUT_DIR defaults to `emei/templates` beside this script's folder.

What it does to each template:
  1. Converts .dot -> .docx with LibreOffice (headless). No macro code survives.
  2. Figure/Table title styles: removes the list numbering that printed the bare word
     "Table"/"Figure" (with no space), and puts a literal "Table "/"Figure " label in front of
     each caption's SEQ field instead, so captions read "Table 1" and lists of tables/figures
     find them.
  3. Drops the <w:attachedTemplate> link to the old add-in path.
  3b. Restores Times New Roman / Arial / Courier New where LibreOffice substituted its
      Liberation fonts, sets US English to Australian English, and bases the Figure/Table
      title styles on Caption so lists of figures/tables find them.
  4. Adds the custom document property EMEIUHB = EMEI | UHB, which the EMEI tab's commands
     read to pick the style set.
  5. Marks the package as a template (.dotx content type) and writes it into a category
     folder (EMEI, UHB, Job Instructions), which File > New shows as categories.
  6. Verifies: no vbaProject part, no macro content types.
"""

import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

DOC_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
TPL_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml"
CUSTOM_CT = "application/vnd.openxmlformats-officedocument.custom-properties+xml"
CUSTOM_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties"

# Old file stem -> (category, new file name). Stems not listed are skipped (backups etc.).
SKIP = re.compile(r"(^~\$|\.orig$)", re.I)


def category(stem: str) -> tuple[str, str]:
    if stem.startswith("Job_Inst"):
        return "Job Instructions", "EMEI"
    if "UHB" in stem:
        return "UHB", "UHB"
    return "EMEI", "EMEI"


def clean_name(stem: str) -> str:
    return re.sub(r"\s+", " ", stem.replace("_ ", "_")).strip()


CAPTION_STYLE = re.compile(r"2[78]b(Table|Figure)Title-(EMEI|UHB)")


FONT_SWAPS = {"Liberation Serif": "Times New Roman", "Liberation Sans": "Arial", "Liberation Mono": "Courier New"}


def fix_fonts_and_language(xml: str) -> str:
    """Undo LibreOffice's substitutions: its metric-compatible fonts back to the Windows
    originals (present on the Defence SOE), and US English back to Australian English."""
    for lo, win in FONT_SWAPS.items():
        xml = xml.replace(f'"{lo}"', f'"{win}"')
    return re.sub(r'(<w:lang [^>]*w:val=")en-US(")', r"\1en-AU\2", xml)


def fix_styles(xml: str) -> str:
    def strip_num(m: re.Match) -> str:
        # Based on Caption (which only adds bold, as these styles already have) so lists of
        # figures/tables and cross-references recognise them as captions.
        s = re.sub(r"<w:numPr>.*?</w:numPr>", "", m.group(0), flags=re.S)
        return re.sub(r'<w:basedOn w:val="[^"]*"/>', '<w:basedOn w:val="Caption"/>', s, count=1)

    return re.sub(
        r'<w:style [^>]*w:styleId="2[78]b(?:Table|Figure)Title-(?:EMEI|UHB)".*?</w:style>',
        strip_num,
        xml,
        flags=re.S,
    )


def fix_document(xml: str) -> tuple[str, int]:
    count = 0

    def label(m: re.Match) -> str:
        nonlocal count
        para = m.group(0)
        sm = re.search(r'<w:pStyle w:val="(2[78]b(Table|Figure)Title-(?:EMEI|UHB))"/>', para)
        if not sm:
            return para
        word = sm.group(2)
        # The paragraph may carry its own copy of the style's label numbering.
        para = re.sub(r"<w:numPr>.*?</w:numPr>", "", para, flags=re.S)
        body = para[para.find("</w:pPr>") + len("</w:pPr>") :] if "</w:pPr>" in para else para
        if re.search(r"<w:t[^>]*>\s*" + word + r"\b", body):
            return para  # already labelled
        run = f'<w:r><w:t xml:space="preserve">{word} </w:t></w:r>'
        count += 1
        if "</w:pPr>" in para:
            return para.replace("</w:pPr>", "</w:pPr>" + run, 1)
        return re.sub(r"(<w:p\b[^>]*>)", r"\1" + run, para, count=1)

    xml = re.sub(r"<w:p\b[^>]*>.*?</w:p>", label, xml, flags=re.S)
    return xml, count


def custom_props(kind: str) -> str:
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" '
        'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
        '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="EMEIUHB">'
        f"<vt:lpwstr>{kind}</vt:lpwstr></property></Properties>"
    )


def postprocess(src_docx: Path, dst: Path, kind: str) -> dict:
    report = {"captions": 0}
    with zipfile.ZipFile(src_docx) as zin:
        parts = {n: zin.read(n) for n in zin.namelist()}

    bad = [n for n in parts if "vba" in n.lower()]
    for n in bad:
        del parts[n]
    report["removed_vba_parts"] = bad

    parts["word/styles.xml"] = fix_styles(parts["word/styles.xml"].decode("utf-8")).encode()
    doc, report["captions"] = fix_document(parts["word/document.xml"].decode("utf-8"))
    parts["word/document.xml"] = doc.encode()

    for n in list(parts):
        if n.startswith("word/") and n.endswith(".xml"):
            parts[n] = fix_fonts_and_language(parts[n].decode("utf-8")).encode()

    if "word/settings.xml" in parts:
        s = parts["word/settings.xml"].decode("utf-8")
        s = re.sub(r"<w:attachedTemplate[^>]*/>", "", s)
        parts["word/settings.xml"] = s.encode()

    ct = parts["[Content_Types].xml"].decode("utf-8").replace(DOC_CT, TPL_CT)
    ct = re.sub(r"<Override[^>]*macroEnabled[^>]*/>", "", ct)
    if "/docProps/custom.xml" not in ct:
        ct = ct.replace("</Types>", f'<Override PartName="/docProps/custom.xml" ContentType="{CUSTOM_CT}"/></Types>')
    parts["[Content_Types].xml"] = ct.encode()

    parts["docProps/custom.xml"] = custom_props(kind).encode()
    rels = parts["_rels/.rels"].decode("utf-8")
    if "custom-properties" not in rels:
        rels = rels.replace("</Relationships>", f'<Relationship Id="rIdEmeiCustom" Type="{CUSTOM_REL}" Target="docProps/custom.xml"/></Relationships>')
    parts["_rels/.rels"] = rels.encode()

    dst.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        zout.writestr("[Content_Types].xml", parts.pop("[Content_Types].xml"))
        for n, data in parts.items():
            zout.writestr(n, data)
    return report


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    src = Path(sys.argv[1])
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parent.parent / "templates"
    dots = sorted(p for p in src.glob("*.dot") if not SKIP.search(p.stem))
    if not dots:
        print(f"no .dot files in {src}")
        return 1
    with tempfile.TemporaryDirectory() as tmp:
        tmpd = Path(tmp)
        for p in dots:
            shutil.copy(p, tmpd / p.name)
        subprocess.run(
            ["soffice", "--headless", "--convert-to", "docx", "--outdir", str(tmpd / "docx")] + [str(tmpd / p.name) for p in dots],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for p in dots:
            conv = tmpd / "docx" / (p.stem + ".docx")
            if not conv.exists():
                print(f"FAILED  {p.name}: LibreOffice produced nothing")
                continue
            cat, kind = category(p.stem)
            dst = out / cat / (clean_name(p.stem) + ".dotx")
            r = postprocess(conv, dst, kind)
            print(f"ok      {cat}/{dst.name}  ({kind}, captions labelled: {r['captions']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
