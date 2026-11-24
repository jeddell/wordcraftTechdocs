//! EMEI / UHB technical-publication commands (fork-only: jeddell/wordcraftTechdocs).
//!
//! The EMEI tab's paragraph buttons: each inserts a new paragraph after the current one in one of
//! the house styles (or, in restyle mode, restyles the selected paragraphs), and refuses a
//! paragraph level that may not follow the current one.
//!
//! Both style sets name their styles `NN Name - EMEI` / `NN Name - UHB` with the same numbers for
//! the same roles, so the rules below work on those numbers:
//!
//! | Numbers | Role |
//! |---|---|
//! | 04 / 05 / 06 | Division (EMEI) or Main (UHB) heading, Group heading, Para heading |
//! | 07–11 | Paragraph, SubPara 1–4 (numbered) |
//! | 12–16 | the same, unnumbered |
//! | 17–21, 22–26 | the numbered and unnumbered paragraphs inside a table |
//!
//! Which set a document uses comes from its `EMEIUHB` custom property (set by the EMEI templates),
//! else from which set its styles belong to. Everything here runs other registered commands
//! (`text.newParagraph`, `para.style`), so undo, macros and automation see ordinary edits.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use wordcraft_doc::Pos;

use super::{sel_result, selected_paths};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session};

/// Insert-new mode (true) or restyle-selection mode (false). Application-wide, like the old
/// toolbar's toggle; it is a working mode, not part of any document.
static INSERT_NEW: AtomicBool = AtomicBool::new(true);

/// Whether the paragraph buttons insert a new paragraph (true) or restyle the selection.
pub fn insert_mode() -> bool {
    INSERT_NEW.load(Ordering::Relaxed)
}

/// The document's style set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Emei,
    Uhb,
}

impl Kind {
    fn suffix(self) -> &'static str {
        match self {
            Kind::Emei => " - EMEI",
            Kind::Uhb => " - UHB",
        }
    }
}

/// The style set of the document in `s`, or `None` when it uses neither.
pub fn kind(s: &Session) -> Option<Kind> {
    match s.doc.custom_prop("EMEIUHB").map(str::trim) {
        Some(v) if v.eq_ignore_ascii_case("UHB") => return Some(Kind::Uhb),
        Some(v) if v.eq_ignore_ascii_case("EMEI") => return Some(Kind::Emei),
        _ => {}
    }
    let has = |k: Kind| s.doc.styles.styles.iter().any(|st| number_of(&st.name, k).is_some());
    if has(Kind::Emei) {
        Some(Kind::Emei)
    } else if has(Kind::Uhb) {
        Some(Kind::Uhb)
    } else {
        None
    }
}

/// The role number of a style name in set `k` (`"08 SubPara 1 - EMEI"` → 8).
fn number_of(name: &str, k: Kind) -> Option<u8> {
    if !name.ends_with(k.suffix()) {
        return None;
    }
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() != 2 {
        return None;
    }
    digits.parse().ok()
}

/// The name of the style with role number `n` in set `k`, if the document has it.
fn style_named(s: &Session, k: Kind, n: u8) -> Option<String> {
    let prefix = format!("{n:02} ");
    s.doc.styles.styles.iter().find(|st| st.name.starts_with(&prefix) && st.name.ends_with(k.suffix())).map(|st| st.name.clone())
}

/// The name of the paragraph style at `p` ("Normal" when the paragraph names none).
fn style_at(s: &Session, p: &Pos) -> String {
    let id = s.doc.para_at(p).and_then(|para| para.props.style.clone()).unwrap_or_else(|| "Normal".to_string());
    s.doc.styles.get(&id).map(|st| st.name.clone()).unwrap_or(id)
}

/// A paragraph level the buttons insert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    MainHeading,
    GroupHeading,
    ParaHeading,
    /// Numbered paragraph, depth 1 (Paragraph) to 5 (SubPara 4).
    Para(u8),
    /// Unnumbered paragraph, depth 1 to 5.
    Plain(u8),
}

impl Level {
    /// The role number of this level, inside a table or not.
    fn number(self, in_table: bool) -> u8 {
        let t = if in_table { 10 } else { 0 };
        match self {
            Level::MainHeading => 4,
            Level::GroupHeading => 5,
            Level::ParaHeading => 6,
            Level::Para(d) => 6 + d + t,
            Level::Plain(d) => 11 + d + t,
        }
    }
}

/// A role number brought to the numbered paragraph scale (04–11), so numbered, unnumbered and
/// table paragraphs compare by depth. `None` for numbers outside 04–26.
fn depth_rank(n: u8) -> Option<u8> {
    match n {
        4..=11 => Some(n),
        12..=16 => Some(n - 5),
        17..=21 => Some(n - 10),
        22..=26 => Some(n - 15),
        _ => None,
    }
}

/// Why `new` may not follow a paragraph whose role number is `current`, or `None` when it may.
/// A paragraph may go at most one level deeper than the one before it; a Paragraph may follow a
/// Group heading directly. Headings may follow anything, except a Para heading directly after
/// another. After a paragraph in another style (a title, Normal) anything may follow.
fn refusal(new: Level, current: Option<u8>) -> Option<&'static str> {
    let cur = current.and_then(depth_rank)?;
    match new {
        Level::MainHeading | Level::GroupHeading => None,
        Level::ParaHeading => (cur == 6).then_some("a Para heading can't follow another Para heading"),
        Level::Para(_) | Level::Plain(_) => {
            let rank = depth_rank(new.number(false)).unwrap_or(0);
            if rank <= cur + 1 || (cur == 5 && rank == 7) { None } else { Some("this paragraph level isn't allowed here") }
        }
    }
}

fn not_emei(s: &Session) -> Option<&'static str> {
    kind(s).is_none().then_some("this document doesn't use the EMEI or UHB styles")
}

/// The selected paragraphs' style, or an error when they don't all share one.
fn one_style(s: &Session, what: &str) -> Result<String, CmdError> {
    let paths = selected_paths(s);
    let story = s.sel.focus.story;
    let mut names = paths.iter().map(|path| style_at(s, &Pos::new(story, path.clone(), 0)));
    let first = names.next().unwrap_or_else(|| style_at(s, &s.sel.focus));
    if names.any(|n| n != first) {
        return Err(CmdError::Failed(format!("{what} needs the selected paragraphs to share one style")));
    }
    Ok(first)
}

/// Insert `level` after the current paragraph, or restyle the selection to it.
fn apply(s: &mut Session, level: Level) -> CmdResult {
    let k = kind(s).ok_or_else(|| CmdError::Disabled("this document doesn't use the EMEI or UHB styles".into()))?;
    let in_table = s.sel.focus.path.cell().is_some();
    let n = level.number(in_table && !matches!(level, Level::MainHeading | Level::GroupHeading | Level::ParaHeading));
    let style = style_named(s, k, n).ok_or_else(|| CmdError::Failed(format!("this document has no style numbered {n:02}")))?;
    if insert_mode() {
        let current = number_of(&style_at(s, &s.sel.focus), k);
        if let Some(why) = refusal(level, current) {
            return Err(CmdError::Failed(why.to_string()));
        }
        // To the end of the current paragraph, then a new paragraph after it.
        let (_, end) = s.sel.ordered();
        let len = s.doc.para_at(&end).map(|p| p.len()).unwrap_or(end.off);
        s.sel = Selection::caret(Pos::new(end.story, end.path.clone(), len));
        s.run("text.newParagraph", &json!({}))?;
    } else {
        one_style(s, "Restyling")?;
    }
    s.run("para.style", &json!({"style": style}))?;
    let mut out = sel_result(s)?;
    if let Some(o) = out.as_object_mut() {
        o.insert("style".into(), json!(style));
    }
    Ok(out)
}

/// Promote (`step` = -1) or demote (+1) the selected paragraphs one level.
fn shift(s: &mut Session, step: i8) -> CmdResult {
    let k = kind(s).ok_or_else(|| CmdError::Disabled("this document doesn't use the EMEI or UHB styles".into()))?;
    let what = if step < 0 { "Promoting" } else { "Demoting" };
    let name = one_style(s, what)?;
    let n = number_of(&name, k).ok_or_else(|| CmdError::Failed(format!("`{name}` isn't an EMEI/UHB paragraph style")))?;
    let ok = if step < 0 { matches!(n, 5..=11 | 13..=16 | 18..=21 | 23..=26) } else { matches!(n, 4..=10 | 12..=15 | 17..=20 | 22..=25) };
    if !ok {
        return Err(CmdError::Failed(format!("`{name}` can't be {} further", if step < 0 { "promoted" } else { "demoted" })));
    }
    let to = n.saturating_add_signed(step);
    let style = style_named(s, k, to).ok_or_else(|| CmdError::Failed(format!("this document has no style numbered {to:02}")))?;
    s.run("para.style", &json!({"style": style}))?;
    let mut out = sel_result(s)?;
    if let Some(o) = out.as_object_mut() {
        o.insert("style".into(), json!(style));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Inserts: Warning / Caution / Note, figures, tables, in-line headings.
// ---------------------------------------------------------------------------------------------

/// The Warning and Caution symbols, compiled in from `emei/assets/wcn/` (see its README).
const WARNING_SYMBOL: &[u8] = include_bytes!("../../../../emei/assets/wcn/Warning.wmf");
const CAUTION_SYMBOL: &[u8] = include_bytes!("../../../../emei/assets/wcn/Caution.wmf");

/// Caption separator between the number and the title: four no-break spaces, as the house style.
const CAPTION_GAP: &str = "\u{a0}\u{a0}\u{a0}\u{a0}";

fn need_kind(s: &Session) -> Result<Kind, CmdError> {
    kind(s).ok_or_else(|| CmdError::Disabled("this document doesn't use the EMEI or UHB styles".into()))
}

fn need_style(s: &Session, k: Kind, n: u8) -> Result<String, CmdError> {
    style_named(s, k, n).ok_or_else(|| CmdError::Failed(format!("this document has no style numbered {n:02}")))
}

/// The style whose name starts with `prefix` (`"27b "`) in set `k`.
fn need_lettered(s: &Session, k: Kind, prefix: &str) -> Result<String, CmdError> {
    s.doc
        .styles
        .styles
        .iter()
        .find(|st| st.name.starts_with(prefix) && st.name.ends_with(k.suffix()))
        .map(|st| st.name.clone())
        .ok_or_else(|| CmdError::Failed(format!("this document has no `{}…{}` style", prefix.trim(), k.suffix())))
}

/// Move the caret to the end of the current paragraph and start a new one after it.
fn new_paragraph_after(s: &mut Session) -> Result<(), CmdError> {
    let (_, end) = s.sel.ordered();
    let len = s.doc.para_at(&end).map(|p| p.len()).unwrap_or(end.off);
    s.sel = Selection::caret(Pos::new(end.story, end.path.clone(), len));
    s.run("text.newParagraph", &json!({}))?;
    Ok(())
}

/// A new paragraph after the current one, unless the current one is empty (then it is reused).
fn fresh_paragraph(s: &mut Session) -> Result<(), CmdError> {
    let empty = s.doc.para_at(&s.sel.focus).is_none_or(|p| p.is_empty());
    if empty {
        let f = s.sel.focus.clone();
        s.sel = Selection::caret(Pos::new(f.story, f.path, 0));
        Ok(())
    } else {
        new_paragraph_after(s)
    }
}

/// The symbols' widths in the legacy templates: Warning 2.84 cm, Caution 3.21 cm.
const WARNING_WIDTH_PT: f32 = 80.6;
const CAUTION_WIDTH_PT: f32 = 90.9;

fn picture_data(bytes: &[u8], width: f32) -> Value {
    json!({ "data": craft_mime::transfer::base64(bytes), "width": width })
}

/// Warning / Caution / Note: the symbol paragraph, then the paragraph for its text.
fn wcn(s: &mut Session, which: &str, v: &Value) -> CmdResult {
    let k = need_kind(s)?;
    let (symbol_style, text_style) = match which {
        "warning" => (need_style(s, k, 29)?, need_lettered(s, k, "30c ")?),
        "caution" => (need_lettered(s, k, "30a ")?, need_lettered(s, k, "30c ")?),
        _ => (need_lettered(s, k, "31a ")?, need_lettered(s, k, "31c ")?),
    };
    new_paragraph_after(s)?;
    s.run("para.style", &json!({"style": symbol_style}))?;
    match which {
        "warning" => s.run("insert.picture", &picture_data(WARNING_SYMBOL, WARNING_WIDTH_PT))?,
        "caution" => s.run("insert.picture", &picture_data(CAUTION_SYMBOL, CAUTION_WIDTH_PT))?,
        _ => s.run("text.insert", &json!({"text": "NOTE"}))?,
    };
    new_paragraph_after(s)?;
    s.run("para.style", &json!({"style": text_style}))?;
    if let Some(text) = v.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
        s.run("text.insert", &json!({"text": text}))?;
    }
    sel_result(s)
}

/// Title case as the old toolbar applied it to captions and in-line headings: each word starts
/// with a capital. The rest of each word is kept as typed, so acronyms (NSN, EMEI) survive.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.trim().chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.push(c);
            if c.is_whitespace() || c == '-' || c == '/' {
                start = true;
            } else if c.is_alphanumeric() {
                start = false;
            }
        }
    }
    out
}

/// Fill the (empty, styled) caption paragraph at the caret: `Figure 3    Title`; in a UHB the
/// number carries its chapter: `Figure 6-3    Title`, as the UHB templates' captions do.
fn write_caption(s: &mut Session, k: Kind, label: &str, title: &str) -> Result<(), CmdError> {
    s.run("text.insert", &json!({"text": format!("{label} ")}))?;
    if k == Kind::Uhb {
        s.run("insert.field", &json!({"instr": " SEQ Chapter \\c \\* ARABIC ", "result": "1"}))?;
        s.run("text.insert", &json!({"text": "-"}))?;
    }
    s.run("insert.field", &json!({"instr": format!(" SEQ {label} \\* ARABIC "), "result": "1"}))?;
    s.run("text.insert", &json!({"text": format!("{CAPTION_GAP}{}", title_case(title))}))?;
    Ok(())
}

fn title_param(v: &Value) -> Result<String, CmdError> {
    let t = v.get("title").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if t.is_empty() {
        return Err(CmdError::Params("`title` is required (the caption text, without the word Figure/Table or its number)".into()));
    }
    Ok(t)
}

/// Insert Graphic: a Figure paragraph (with the picture, when one is given) and its caption below.
/// Without a picture the caret is left in the empty Figure paragraph, where the app's picture
/// chooser then inserts the chosen file.
fn figure(s: &mut Session, v: &Value) -> CmdResult {
    let k = need_kind(s)?;
    let title = title_param(v)?;
    let figure_style = need_lettered(s, k, "28a ")?;
    let caption_style = need_lettered(s, k, "28b ")?;
    fresh_paragraph(s)?;
    s.run("para.style", &json!({"style": figure_style}))?;
    let figure_at = s.sel.focus.clone();
    let has_picture = v.get("path").is_some() || v.get("data").is_some();
    if has_picture {
        let mut pic = json!({});
        for key in ["path", "data", "width", "alt"] {
            if let Some(x) = v.get(key) {
                pic[key] = x.clone();
            }
        }
        s.run("insert.picture", &pic)?;
    }
    new_paragraph_after(s)?;
    s.run("para.style", &json!({"style": caption_style}))?;
    write_caption(s, k, "Figure", &title)?;
    if !has_picture {
        s.sel = Selection::caret(Pos::new(figure_at.story, figure_at.path, 0));
    }
    let mut out = sel_result(s)?;
    if let Some(o) = out.as_object_mut() {
        o.insert("awaitingPicture".into(), json!(!has_picture));
    }
    Ok(out)
}

const CM: f32 = 72.0 / 2.54;

/// Create Table: the Table title above, then the table, its cells in the Table style.
fn table(s: &mut Session, v: &Value) -> CmdResult {
    let k = need_kind(s)?;
    let title = title_param(v)?;
    let rows = v.get("rows").and_then(Value::as_u64).unwrap_or(2).clamp(1, 1000);
    let cols = v.get("cols").and_then(Value::as_u64).unwrap_or(3).clamp(1, 63);
    let title_style = need_lettered(s, k, "27b ")?;
    let cell_style = need_lettered(s, k, "27a ")?;
    fresh_paragraph(s)?;
    s.run("para.style", &json!({"style": title_style}))?;
    write_caption(s, k, "Table", &title)?;
    new_paragraph_after(s)?;
    s.run("para.style", &json!({"style": cell_style}))?;
    s.run("insert.table", &json!({"rows": rows, "cols": cols}))?;
    let first_cell = s.sel.clone();

    // Whole-table settings.
    s.run("table.selectTable", &json!({}))?;
    s.run("para.style", &json!({"style": cell_style}))?;
    // Single-line grid borders (½ pt), as EMEI/UHB tables have.
    s.run("table.borders", &json!({"kind": "all", "width": 0.5}))?;
    let mut props = json!({});
    match v.get("align").and_then(Value::as_str).unwrap_or("left") {
        "centre" | "center" => props["align"] = json!("center"),
        "right" => props["align"] = json!("right"),
        _ => {
            props["align"] = json!("left");
            if let Some(cm) = v.get("indent").and_then(Value::as_f64) {
                props["indent"] = json!(cm as f32 * CM);
            }
        }
    }
    match v.get("rowHeightRule").and_then(Value::as_str) {
        Some("atLeast") | Some("exact") => {
            let cm = v.get("rowHeight").and_then(Value::as_f64).unwrap_or(0.6) as f32;
            props["rowHeight"] = json!(cm * CM);
            props["rowHeightRule"] = json!(v["rowHeightRule"].clone());
        }
        _ => {}
    }
    s.run("table.properties", &props)?;

    // Column widths (cm), left to right; missing or empty entries keep the even split.
    if let Some(widths) = v.get("widths").and_then(Value::as_array) {
        s.sel = first_cell.clone();
        for (i, w) in widths.iter().take(cols as usize).enumerate() {
            if i > 0 {
                s.run("text.tab", &json!({}))?;
            }
            if let Some(cm) = w.as_f64().filter(|cm| *cm > 0.0) {
                s.run("table.properties", &json!({"columnWidth": cm as f32 * CM}))?;
            }
        }
    }
    s.sel = first_cell;
    sel_result(s)
}

/// In-line heading: a bold Arial title (title case, full stop, two spaces) at the start of the
/// current Paragraph..SubPara 3; in restyle mode, the selection is made bold Arial instead.
fn inline_heading(s: &mut Session, v: &Value) -> CmdResult {
    let k = need_kind(s)?;
    if !insert_mode() {
        if s.sel.is_collapsed() {
            return Err(CmdError::Failed("select the words to make an in-line heading".into()));
        }
        s.run("format.bold", &json!({"value": true}))?;
        s.run("format.italic", &json!({"value": false}))?;
        s.run("format.font", &json!({"name": "Arial"}))?;
        return sel_result(s);
    }
    let current = number_of(&style_at(s, &s.sel.focus), k);
    if !matches!(current, Some(7..=10)) {
        return Err(CmdError::Failed("an in-line heading goes at the start of a Paragraph or SubPara 1–3; put the cursor in one".into()));
    }
    let mut title = title_case(&title_param(v)?);
    if !title.ends_with('.') {
        title.push('.');
    }
    let at = s.sel.focus.clone();
    let start = Pos::new(at.story, at.path.clone(), 0);
    s.sel = Selection::caret(start.clone());
    s.run("text.insert", &json!({"text": format!("{title}  ")}))?;
    s.sel = Selection { anchor: start.clone(), focus: Pos::new(at.story, at.path.clone(), title.len()) };
    s.run("format.bold", &json!({"value": true}))?;
    s.run("format.italic", &json!({"value": false}))?;
    s.run("format.font", &json!({"name": "Arial"}))?;
    // Back where the person was, moved along by the heading.
    s.sel = Selection::caret(Pos::new(at.story, at.path, at.off + title.len() + 2));
    sel_result(s)
}

fn toggle_mode(_s: &mut Session, v: &Value) -> CmdResult {
    let next = v.get("insertNew").and_then(Value::as_bool).unwrap_or(!insert_mode());
    INSERT_NEW.store(next, Ordering::Relaxed);
    Ok(json!({"insertNew": next}))
}

fn info(s: &mut Session, _: &Value) -> CmdResult {
    let k = kind(s);
    Ok(json!({
        "kind": k.map(|k| if k == Kind::Uhb { "UHB" } else { "EMEI" }),
        "insertNew": insert_mode(),
        "style": style_at(s, &s.sel.focus),
        "inTable": s.sel.focus.path.cell().is_some(),
    }))
}

macro_rules! level_cmd {
    ($id:literal, $label:literal, $level:expr) => {
        CommandSpec::new($id, $label, "EMEI › Paragraphs", |s, _| apply(s, $level)).when(not_emei)
    };
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        level_cmd!("emei.mainHeading", "Division / Main Heading", Level::MainHeading),
        level_cmd!("emei.groupHeading", "Group Heading", Level::GroupHeading),
        level_cmd!("emei.paraHeading", "Para Heading", Level::ParaHeading),
        level_cmd!("emei.para1", "Paragraph", Level::Para(1)),
        level_cmd!("emei.para2", "SubPara 1", Level::Para(2)),
        level_cmd!("emei.para3", "SubPara 2", Level::Para(3)),
        level_cmd!("emei.para4", "SubPara 3", Level::Para(4)),
        level_cmd!("emei.para5", "SubPara 4", Level::Para(5)),
        level_cmd!("emei.plain1", "Para - No Number", Level::Plain(1)),
        level_cmd!("emei.plain2", "SubPara 1 - No Number", Level::Plain(2)),
        level_cmd!("emei.plain3", "SubPara 2 - No Number", Level::Plain(3)),
        level_cmd!("emei.plain4", "SubPara 3 - No Number", Level::Plain(4)),
        level_cmd!("emei.plain5", "SubPara 4 - No Number", Level::Plain(5)),
        CommandSpec::new("emei.promote", "Promote Level", "EMEI › Paragraphs", |s, _| shift(s, -1)).when(not_emei),
        CommandSpec::new("emei.demote", "Demote Level", "EMEI › Paragraphs", |s, _| shift(s, 1)).when(not_emei),
        CommandSpec::new("emei.warning", "Warning", "EMEI › Inserts", |s, v| wcn(s, "warning", v))
            .params(r#"{"text"?: string}"#)
            .when(not_emei),
        CommandSpec::new("emei.caution", "Caution", "EMEI › Inserts", |s, v| wcn(s, "caution", v))
            .params(r#"{"text"?: string}"#)
            .when(not_emei),
        CommandSpec::new("emei.note", "Note", "EMEI › Inserts", |s, v| wcn(s, "note", v)).params(r#"{"text"?: string}"#).when(not_emei),
        CommandSpec::new("emei.figure", "Insert Graphic", "EMEI › Inserts", figure)
            .params(
                r#"{"title": string, "path"?: string, "data"?: base64, "width"?: pt, "alt"?: string} → selection, "awaitingPicture" (no picture given: the caret is in the empty Figure paragraph)"#,
            )
            .when(not_emei),
        CommandSpec::new("emei.table", "Create Table", "EMEI › Inserts", table)
            .params(
                r#"{"title": string, "rows"?: n, "cols"?: n, "align"?: "left|centre|right", "indent"?: cm, "rowHeightRule"?: "auto|atLeast|exact", "rowHeight"?: cm, "widths"?: [cm…]}"#,
            )
            .when(not_emei),
        CommandSpec::new("emei.inlineHeading", "In-line Heading", "EMEI › Inserts", inline_heading)
            .params(r#"{"title": string} (insert mode; restyle mode makes the selection bold Arial)"#)
            .when(not_emei),
        CommandSpec::new("emei.toggleMode", "Insert New / Restyle", "EMEI › Paragraphs", toggle_mode)
            .params(r#"{"insertNew"?: bool (default: switch)} → {"insertNew"}"#)
            .pure(),
        CommandSpec::new("emei.info", "EMEI Status", "EMEI", info)
            .params(r#"{} → {"kind": "EMEI|UHB"|null, "insertNew", "style", "inTable"}"#)
            .pure(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_goes_at_most_one_level_deeper() {
        // After a Paragraph (07): SubPara 1 is fine, SubPara 2 is two levels deeper.
        assert!(refusal(Level::Para(2), Some(7)).is_none());
        assert!(refusal(Level::Para(3), Some(7)).is_some());
        // Unnumbered and table paragraphs compare by depth with numbered ones.
        assert!(refusal(Level::Plain(2), Some(7)).is_none());
        assert!(refusal(Level::Para(2), Some(12)).is_none());
        assert!(refusal(Level::Para(3), Some(17)).is_some());
        // Shallower is always fine.
        assert!(refusal(Level::Para(1), Some(11)).is_none());
    }

    #[test]
    fn headings_and_unknown_styles() {
        // A Paragraph may follow a Group heading directly, but not a Division heading.
        assert!(refusal(Level::Para(1), Some(5)).is_none());
        assert!(refusal(Level::Para(1), Some(4)).is_some());
        // Two Para headings in a row are refused; other headings go anywhere.
        assert!(refusal(Level::ParaHeading, Some(6)).is_some());
        assert!(refusal(Level::GroupHeading, Some(11)).is_none());
        // After a title or Normal, anything goes.
        assert!(refusal(Level::Para(3), None).is_none());
        assert!(refusal(Level::Para(3), Some(2)).is_none());
    }

    #[test]
    fn role_numbers_come_from_the_set_suffix() {
        assert_eq!(number_of("08 SubPara 1 - EMEI", Kind::Emei), Some(8));
        assert_eq!(number_of("08 SubPara 1 - EMEI", Kind::Uhb), None);
        assert_eq!(number_of("27a Table - UHB", Kind::Uhb), Some(27));
        assert_eq!(number_of("Normal", Kind::Emei), None);
    }
}
