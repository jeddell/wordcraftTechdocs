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
