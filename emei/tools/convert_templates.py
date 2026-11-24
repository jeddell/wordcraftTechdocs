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


LEVEL_STYLE = re.compile(r'(<w:style [^>]*w:styleId="([^"]+)".*?<w:name w:val="(\d\d)[a-c]? [^"]* - (?:EMEI|UHB)".*?</w:style>)', re.S)


def list_level(number: int) -> int | None:
    """The list level a numbered paragraph style sits on: Paragraph..SubPara 4 are levels 0-4
    (1. / a. / (1) / (a) / i.), in body text (07-11) and in tables (17-21). LibreOffice's
    conversion puts them all on level 0, which numbers every level 1., 2., 3."""
    if 7 <= number <= 11:
        return number - 7
    if 17 <= number <= 21:
        return number - 17
    return None


# Unnumbered headings that the old templates parked on a blank list level.
UNNUMBERED_HEADINGS = {4, 5, 6, 36}


def fix_list_levels(xml: str) -> tuple[str, set[str]]:
    """Put each numbered style back on its list level; take the unnumbered headings out of the
    list. Returns the styled xml and the ids of the styles whose numbering this decides."""
    ids: set[str] = set()

    def fix(m: re.Match) -> str:
        block, sid, num = m.group(1), m.group(2), int(m.group(3))
        if "<w:numPr>" not in block:
            return block
        lvl = list_level(num)
        if lvl is not None:
            ids.add(sid)
            return re.sub(r'(<w:numPr>\s*<w:ilvl w:val=")\d(")', rf"\g<1>{lvl}\2", block, count=1)
        if num in UNNUMBERED_HEADINGS:
            ids.add(sid)
            return re.sub(r"<w:numPr>.*?</w:numPr>", "", block, count=1, flags=re.S)
        return block

    return LEVEL_STYLE.sub(fix, xml), ids


# The paragraph list's indents (twips) as nearly every legacy template has them: Paragraph's
# number at the margin, then each SubPara level 1 cm (567) further in with a 1 cm hanging number.
CANON_INDENTS = {0: (0, 0), 1: (1247, 567), 2: (1814, 567), 3: (2381, 567), 4: (2948, 567)}


def fix_list_indents(styles: str, numbering: str) -> str:
    """Give the body-paragraph list (the one 07 Paragraph uses) the canonical level indents; a
    few conversions lost them (every level at 0)."""
    m = re.search(r'<w:style [^>]*w:styleId="07Paragraph-(?:EMEI|UHB)".*?<w:numId w:val="(\d+)"', styles, re.S)
    if not m:
        return numbering
    am = re.search(r'<w:num w:numId="%s"[^>]*>.*?<w:abstractNumId w:val="(\d+)"' % m.group(1), numbering, re.S)
    if not am:
        return numbering

    def fix_abstract(a: re.Match) -> str:
        block = a.group(0)
        for lvl, (start, hanging) in CANON_INDENTS.items():
            block = re.sub(
                r'(<w:lvl w:ilvl="%d".*?)<w:ind [^>]*/>' % lvl,
                rf'\g<1><w:ind w:start="{start}" w:hanging="{hanging}"/>',
                block,
                count=1,
                flags=re.S,
            )
        return block

    return re.sub(r'<w:abstractNum [^>]*w:abstractNumId="%s".*?</w:abstractNum>' % am.group(1), fix_abstract, numbering, count=1, flags=re.S)


def drop_direct_numbering(xml: str, style_ids: set[str]) -> str:
    """Paragraphs in those styles lose direct numbering and indent copies, so the style's list
    level (and its indents) apply."""

    def fix(m: re.Match) -> str:
        para = m.group(0)
        sm = re.search(r'<w:pStyle w:val="([^"]+)"/>', para)
        if sm and sm.group(1) in style_ids:
            para = re.sub(r"<w:numPr>.*?</w:numPr>", "", para, count=1, flags=re.S)
            ppr = re.search(r"<w:pPr>.*?</w:pPr>", para, re.S)
            if ppr:
                para = para.replace(ppr.group(0), re.sub(r"<w:ind [^>]*/>", "", ppr.group(0), count=1))
        return para

    return re.sub(r"<w:p\b[^>]*>.*?</w:p>", fix, xml, flags=re.S)


# Schema order of a numbering level's children (ECMA-376 CT_Lvl). LibreOffice writes pStyle before
# numFmt, which a strict reader (Word's validator) rejects.
LVL_ORDER = ["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff", "lvlText", "lvlPicBulletId", "legacy", "lvlJc", "pPr", "rPr"]


def order_levels(numbering: str) -> str:
    def fix(m: re.Match) -> str:
        head, body, tail = m.group(1), m.group(2), m.group(3)
        # Split the level's children (self-closing or with content, one level of nesting deep).
        found = list(re.finditer(r"<w:(\w+)\b(?:[^>]*/>|[^>]*>.*?</w:\1>)", body, re.S))
        kids = [k.group(1) for k in found]
        parts = [k.group(0) for k in found]
        if re.sub(r"\s+", "", "".join(parts)) != re.sub(r"\s+", "", body):
            return m.group(0)  # something unexpected in this level: leave it alone
        rank = {n: i for i, n in enumerate(LVL_ORDER)}
        ordered = [p for _, p in sorted(zip(kids, parts), key=lambda kp: rank.get(kp[0], len(LVL_ORDER)))]
        return head + "".join(ordered) + tail

    return re.sub(r"(<w:lvl\b[^>]*>)(.*?)(</w:lvl>)", fix, numbering, flags=re.S)


def drop_orphan_bookmarks(xml: str) -> str:
    """Bookmark starts with no end (the conversion leaves duplicates in headers and footers); a
    saved document would repeat their ids."""
    ends = set(re.findall(r'<w:bookmarkEnd [^>]*w:id="(\d+)"', xml))
    return re.sub(r'<w:bookmarkStart [^>]*w:id="(\d+)"[^>]*/>', lambda m: m.group(0) if m.group(1) in ends else "", xml)


def sect_pr_last(xml: str) -> str:
    """A section break's settings (sectPr) go last in its paragraph's properties (only a revision
    mark may follow); LibreOffice writes them first."""

    def fix(m: re.Match) -> str:
        ppr = m.group(0)
        sp = re.search(r"<w:sectPr\b.*?</w:sectPr>", ppr, re.S)
        if not sp:
            return ppr
        rest = ppr.replace(sp.group(0), "", 1)
        at = rest.find("<w:pPrChange")
        if at < 0:
            at = rest.rfind("</w:pPr>")
        return rest[:at] + sp.group(0) + rest[at:]

    return re.sub(r"<w:pPr>.*?</w:pPr>", fix, xml, flags=re.S)


MEDIA_TYPES = {"wmf": "image/x-wmf", "emf": "image/x-emf", "png": "image/png", "jpeg": "image/jpeg", "jpg": "image/jpeg", "gif": "image/gif", "bmp": "image/bmp", "tif": "image/tiff", "tiff": "image/tiff"}


def declare_media(ct: str, names: list[str]) -> str:
    """Every media file's extension declared in [Content_Types].xml, as Word requires."""
    for ext in sorted({n.rsplit(".", 1)[-1].lower() for n in names if n.startswith("word/media/") and "." in n}):
        if ext in MEDIA_TYPES and f'Extension="{ext}"' not in ct:
            ct = ct.replace("</Types>", f'<Default Extension="{ext}" ContentType="{MEDIA_TYPES[ext]}"/></Types>')
    return ct


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
        # A chapter number in a caption repeats the chapter's (\c); the conversion dropped
        # the switch the originals wrote as `SEQ Chapter\c`, so every caption would count up.
        para = re.sub(r"(<w:instrText[^>]*>\s*SEQ Chapter)(?![^<]*\\c)", r"\1 \\c", para)
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

    styles, level_ids = fix_list_levels(fix_styles(parts["word/styles.xml"].decode("utf-8")))
    parts["word/styles.xml"] = styles.encode()
    if "word/numbering.xml" in parts:
        parts["word/numbering.xml"] = fix_list_indents(styles, parts["word/numbering.xml"].decode("utf-8")).encode()
    doc, report["captions"] = fix_document(parts["word/document.xml"].decode("utf-8"))
    doc = sect_pr_last(drop_direct_numbering(doc, level_ids))
    parts["word/document.xml"] = doc.encode()

    for n in list(parts):
        if n.startswith("word/") and n.endswith(".xml"):
            parts[n] = drop_orphan_bookmarks(fix_fonts_and_language(parts[n].decode("utf-8"))).encode()
    if "word/numbering.xml" in parts:
        parts["word/numbering.xml"] = order_levels(parts["word/numbering.xml"].decode("utf-8")).encode()

    if "word/settings.xml" in parts:
        s = parts["word/settings.xml"].decode("utf-8")
        s = re.sub(r"<w:attachedTemplate[^>]*/>", "", s)
        parts["word/settings.xml"] = s.encode()

    ct = parts["[Content_Types].xml"].decode("utf-8").replace(DOC_CT, TPL_CT)
    ct = re.sub(r"<Override[^>]*macroEnabled[^>]*/>", "", ct)
    if "/docProps/custom.xml" not in ct:
        ct = ct.replace("</Types>", f'<Override PartName="/docProps/custom.xml" ContentType="{CUSTOM_CT}"/></Types>')
    ct = declare_media(ct, list(parts))
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
