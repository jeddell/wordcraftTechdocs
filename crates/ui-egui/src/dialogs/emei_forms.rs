//! The EMEI tab's dialogs (fork-only: jeddell/wordcraftTechdocs): Insert Graphic, Create Table and
//! In-line Heading. Each collects its values and runs the matching `emei.*` engine command, so the
//! same inserts work from the CLI and automation without these dialogs.

use egui::Context;
use serde_json::{Value, json};

use craft::ui::contract::Bind;
use craft::ui::dialog::DialogId;
use craft::ui::patterns::form_dialogs::{DialogFields, HostedParamDialog, Param, ParamDialog, ParamKind, ParamResult};
use craft::ui::runtime::env::Label;

use super::host::{DialogHooks, Settle, open_with};
use crate::WordApp;

/// Opens `emeiFigure`, `emeiTable` or `emeiInlineHeading`; `None` for any other name.
pub(crate) fn open(app: &mut WordApp, name: &str, _params: &Value, parent: Option<DialogId>) -> Option<DialogId> {
    let ctx = app.ctx.clone().unwrap_or_default();
    match name {
        "emeiFigure" => Some(open_figure(app, &ctx, parent)),
        "emeiTable" => Some(open_table(app, &ctx, parent)),
        "emeiInlineHeading" => Some(open_inline_heading(app, &ctx, parent)),
        "emeiChapter" => Some(open_chapter(app, &ctx, parent)),
        _ => None,
    }
}

/// Takes the filed OK of the dialog `id`, runs `run` on its values and answers the dialog with the
/// result (a refusal is shown in the dialog, which stays open).
fn settle_with(app: &mut WordApp, id: DialogId, run: impl FnOnce(&mut WordApp, &ParamResult) -> Result<(), String>) -> Settle {
    let Some(dialog) = app.dialog_host.get_mut_by_id::<HostedParamDialog>(id) else {
        return Settle::Done;
    };
    let Some(result) = dialog.take_ok() else {
        return Settle::Keep;
    };
    let answer = run(app, &result).map_err(Label::from);
    if let Some(dialog) = app.dialog_host.get_mut_by_id::<HostedParamDialog>(id) {
        dialog.answer_ok(answer);
    }
    Settle::Keep
}

fn open_hosted(
    app: &mut WordApp,
    parent: Option<DialogId>,
    dialog: HostedParamDialog,
    settle: fn(&mut WordApp, &Context, DialogId) -> Settle,
) -> DialogId {
    open_with(app, parent, dialog.held_ok(), DialogHooks { prepare: None, settle })
}

fn title_text(result: &ParamResult) -> Result<String, String> {
    let t = result.fields.text("title").unwrap_or_default().trim().to_string();
    if t.is_empty() { Err(tl!("Type a title.").to_string()) } else { Ok(t) }
}

// Insert Graphic -------------------------------------------------------------------------------

fn open_figure(app: &mut WordApp, ctx: &Context, parent: Option<DialogId>) -> DialogId {
    let fields = DialogFields::from_pairs([("title", "")]);
    let dialog = HostedParamDialog::new(ctx, "emeiFigure", fields, |fields| {
        let params = vec![
            Param::new("title", tl!("Figure title (without the word Figure or its number)").to_string(), ParamKind::text(""))
                .max_chars(250)
                .field_chars(50),
        ];
        ParamDialog::command("emeiFigure", tl!("Insert Graphic").to_string(), params)
            .ok_label(Label::from(tl!("Choose Picture…")))
            .values(Bind::bound(fields))
            .alt_resets(false)
    });
    open_hosted(app, parent, dialog, settle_figure)
}

fn settle_figure(app: &mut WordApp, _: &Context, id: DialogId) -> Settle {
    settle_with(app, id, |app, result| {
        let title = title_text(result)?;
        app.run("emei.figure", json!({"title": title})).map(|_| ())?;
        // The Figure paragraph is ready with the caret in it: the picture chooser inserts there.
        app.pick_picture();
        Ok(())
    })
}

// Create Table ---------------------------------------------------------------------------------

fn open_table(app: &mut WordApp, ctx: &Context, parent: Option<DialogId>) -> DialogId {
    let fields = DialogFields::from_pairs([("title", "")]);
    let dialog = HostedParamDialog::new(ctx, "emeiTable", fields, |fields| {
        let align = vec![
            ("left".to_string(), Label::from(tl!("Left").to_string())),
            ("centre".to_string(), Label::from(tl!("Centre").to_string())),
            ("right".to_string(), Label::from(tl!("Right").to_string())),
        ];
        let height = vec![
            ("auto".to_string(), Label::from(tl!("Auto").to_string())),
            ("atLeast".to_string(), Label::from(tl!("At least").to_string())),
            ("exact".to_string(), Label::from(tl!("Exactly").to_string())),
        ];
        let params = vec![
            Param::new("title", tl!("Table title (without the word Table or its number)").to_string(), ParamKind::text(""))
                .max_chars(250)
                .field_chars(50),
            Param::new("cols", tl!("Number of columns").to_string(), ParamKind::int(Some(1), Some(15), 3)),
            Param::new("rows", tl!("Number of rows").to_string(), ParamKind::int(Some(1), Some(1000), 3)),
            Param::new("align", tl!("Table position").to_string(), ParamKind::radio(align, Some("left".into()))).options_in_row(),
            Param::new("indent", tl!("Indent from left (cm)").to_string(), ParamKind::number(Some(0.0), Some(20.0), 0.0, 0.1, 2))
                .enabled_when_in("align", ["left"]),
            Param::new("rowHeightRule", tl!("Row height").to_string(), ParamKind::choice(height, Some("auto".into()))),
            Param::new("rowHeight", tl!("Height (cm)").to_string(), ParamKind::number(Some(0.1), Some(20.0), 0.6, 0.1, 2))
                .enabled_when_in("rowHeightRule", ["atLeast", "exact"]),
            Param::new("widths", tl!("Column widths in cm, separated by commas (blank: equal widths)").to_string(), ParamKind::text(""))
                .max_chars(200)
                .field_chars(30),
        ];
        ParamDialog::command("emeiTable", tl!("Create Table").to_string(), params)
            .ok_label(Label::from(tl!("Create")))
            .values(Bind::bound(fields))
            .alt_resets(false)
    });
    open_hosted(app, parent, dialog, settle_table)
}

/// "3, 5.5, , 4" → [3, 5.5, 0, 4] (0 keeps that column's even width); an unreadable entry is an error.
fn parse_widths(text: &str) -> Result<Vec<f64>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    text.split(',')
        .map(|w| {
            let w = w.trim();
            if w.is_empty() { Ok(0.0) } else { w.parse::<f64>().map_err(|_| format!("`{w}` isn't a width in centimetres")) }
        })
        .collect()
}

fn settle_table(app: &mut WordApp, _: &Context, id: DialogId) -> Settle {
    settle_with(app, id, |app, result| {
        let f = &result.fields;
        let title = title_text(result)?;
        let widths = parse_widths(f.text("widths").unwrap_or_default())?;
        let mut params = json!({
            "title": title,
            "rows": f.int("rows").unwrap_or(3),
            "cols": f.int("cols").unwrap_or(3),
            "align": f.text("align").unwrap_or("left"),
            "indent": f.number("indent").unwrap_or(0.0),
            "rowHeightRule": f.text("rowHeightRule").unwrap_or("auto"),
            "rowHeight": f.number("rowHeight").unwrap_or(0.6),
        });
        if !widths.is_empty() {
            params["widths"] = json!(widths);
        }
        app.run("emei.table", params).map(|_| ())
    })
}

// In-line Heading ------------------------------------------------------------------------------

fn open_inline_heading(app: &mut WordApp, ctx: &Context, parent: Option<DialogId>) -> DialogId {
    let fields = DialogFields::from_pairs([("title", "")]);
    let dialog = HostedParamDialog::new(ctx, "emeiInlineHeading", fields, |fields| {
        let params = vec![Param::new("title", tl!("Paragraph title").to_string(), ParamKind::text("")).max_chars(120).field_chars(40)];
        ParamDialog::command("emeiInlineHeading", tl!("In-line Heading").to_string(), params)
            .ok_label(Label::from(tl!("Insert")))
            .values(Bind::bound(fields))
            .alt_resets(false)
    });
    open_hosted(app, parent, dialog, settle_inline_heading)
}

fn settle_inline_heading(app: &mut WordApp, _: &Context, id: DialogId) -> Settle {
    settle_with(app, id, |app, result| {
        let title = title_text(result)?;
        app.run("emei.inlineHeading", json!({"title": title})).map(|_| ())
    })
}

// Insert Chapter (UHB) ----------------------------------------------------------------------

fn open_chapter(app: &mut WordApp, ctx: &Context, parent: Option<DialogId>) -> DialogId {
    let fields = DialogFields::from_pairs([("title", "")]);
    let dialog = HostedParamDialog::new(ctx, "emeiChapter", fields, |fields| {
        let params = vec![
            Param::new("title", tl!("Chapter title").to_string(), ParamKind::text("")).max_chars(120).field_chars(40),
            Param::new("restart", tl!("Start this file's chapter numbers at a set number").to_string(), ParamKind::bool(false)),
            Param::new("number", tl!("First chapter number").to_string(), ParamKind::int(Some(1), Some(99), 1)).enabled_when_in("restart", [true]),
        ];
        ParamDialog::command("emeiChapter", tl!("Insert Chapter").to_string(), params)
            .ok_label(Label::from(tl!("Insert")))
            .values(Bind::bound(fields))
            .alt_resets(false)
    });
    open_hosted(app, parent, dialog, settle_chapter)
}

fn settle_chapter(app: &mut WordApp, _: &Context, id: DialogId) -> Settle {
    settle_with(app, id, |app, result| {
        let f = &result.fields;
        let mut params = json!({"title": title_text(result)?});
        if f.bool("restart").unwrap_or(false) {
            params["number"] = json!(f.int("number").unwrap_or(1));
        }
        app.run("emei.chapter", params).map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::parse_widths;

    #[test]
    fn column_widths_list() {
        assert!(parse_widths("  ").unwrap().is_empty());
        let w = parse_widths("3, 5.5, , 4").unwrap();
        assert_eq!(w.len(), 4);
        assert!(w[2] == 0.0 && w[1] > w[0]);
        assert!(parse_widths("3, wide").is_err());
    }
}
