//! The EMEI tab (fork-only: jeddell/wordcraftTechdocs): the EMEI/UHB house-style paragraph
//! buttons. Every button runs an `emei.*` engine command, so the same actions work from the CLI,
//! scripts and automation. See `crates/engine/src/cmd/emei.rs`.

use craft::behavior::IconId;
use craft::keymap::CommandRef;
use craft::menu::ribbon::{RibbonGroup, RibbonRow};
use craft::ui::buttons::{Arrangement, Button, ToggleButton};
use craft::ui::contract::{Gap, Props as _, Show as _, ShowContainer as _};
use craft::ui::layout_components::Stack;
use serde_json::json;
use wordcraft_engine::cmd::emei;

use super::WordApp;

/// A small text button in a stack.
fn small(ui: &mut egui::Ui, id: &'static str, label: String) {
    Button::command(CommandRef::new(id)).label(label).show(ui);
}

/// The EMEI tab.
pub(super) fn emei_tab(row: &mut RibbonRow<'_>, app: &mut WordApp) {
    let insert = emei::insert_mode();
    let uhb = emei::kind(&app.session) == Some(emei::Kind::Uhb);

    row.group(RibbonGroup::new("emei.mode", tl!("Mode").to_string(), IconId::new("styles").into()), |ui| {
        let label = if insert { tl!("Insert") } else { tl!("Restyle") };
        ToggleButton::command(CommandRef::new("emei.toggleMode")).label(label.to_string()).icon(IconId::new("styles")).selected(insert).show(ui);
    });

    row.group(RibbonGroup::new("emei.headings", tl!("Headings").to_string(), IconId::new("outline").into()), |ui| {
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            let main = if uhb { tl!("Main Heading") } else { tl!("Division Heading") };
            small(ui, "emei.mainHeading", main.to_string());
            small(ui, "emei.groupHeading", tl!("Group Heading").to_string());
            small(ui, "emei.paraHeading", tl!("Para Heading").to_string());
        });
    });

    row.group(RibbonGroup::new("emei.numbered", tl!("Numbered").to_string(), IconId::new("multilevel").into()), |ui| {
        Button::command(CommandRef::new("emei.para1"))
            .label(tl!("Paragraph").to_string())
            .icon(IconId::new("multilevel"))
            .arrangement(Arrangement::Stacked)
            .show(ui);
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.para2", tl!("SubPara 1").to_string());
            small(ui, "emei.para3", tl!("SubPara 2").to_string());
        });
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.para4", tl!("SubPara 3").to_string());
            small(ui, "emei.para5", tl!("SubPara 4").to_string());
        });
    });

    row.group(RibbonGroup::new("emei.unnumbered", tl!("No Number").to_string(), IconId::new("indent").into()), |ui| {
        Button::command(CommandRef::new("emei.plain1"))
            .label(tl!("Para").to_string())
            .icon(IconId::new("indent"))
            .arrangement(Arrangement::Stacked)
            .show(ui);
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.plain2", tl!("SubPara 1").to_string());
            small(ui, "emei.plain3", tl!("SubPara 2").to_string());
        });
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.plain4", tl!("SubPara 3").to_string());
            small(ui, "emei.plain5", tl!("SubPara 4").to_string());
        });
    });

    row.group(RibbonGroup::new("emei.inserts", tl!("Inserts").to_string(), IconId::new("picture").into()), |ui| {
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.warning", tl!("Warning").to_string());
            small(ui, "emei.caution", tl!("Caution").to_string());
            small(ui, "emei.note", tl!("Note").to_string());
        });
        Button::command(CommandRef::with_params("ui.dialog", json!({"name": "emeiFigure"})))
            .label(tl!("Graphic").to_string())
            .icon(IconId::new("picture"))
            .arrangement(Arrangement::Stacked)
            .show(ui);
        Button::command(CommandRef::with_params("ui.dialog", json!({"name": "emeiTable"})))
            .label(tl!("Table").to_string())
            .icon(IconId::new("table"))
            .arrangement(Arrangement::Stacked)
            .show(ui);
        // In restyle mode the in-line heading formats the selection, so it needs no title.
        let inline =
            if insert { CommandRef::with_params("ui.dialog", json!({"name": "emeiInlineHeading"})) } else { CommandRef::new("emei.inlineHeading") };
        Button::command(inline).label(tl!("In-line\nHeading").to_string()).icon(IconId::new("caption")).arrangement(Arrangement::Stacked).show(ui);
    });

    row.group(RibbonGroup::new("emei.document", tl!("Document").to_string(), IconId::new("toc").into()), |ui| {
        Button::command(CommandRef::new("emei.toc"))
            .label(tl!("Contents").to_string())
            .icon(IconId::new("toc"))
            .arrangement(Arrangement::Stacked)
            .show(ui);
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            small(ui, "emei.listFigures", tl!("List of Figures").to_string());
            small(ui, "emei.listTables", tl!("List of Tables").to_string());
            Button::command(CommandRef::new("emei.updateAll")).label(tl!("Update All").to_string()).icon(IconId::new("updateTable")).show(ui);
        });
        if uhb {
            Button::command(CommandRef::with_params("ui.dialog", json!({"name": "emeiChapter"})))
                .label(tl!("Insert\nChapter").to_string())
                .icon(IconId::new("header"))
                .arrangement(Arrangement::Stacked)
                .show(ui);
        }
        ToggleButton::command(CommandRef::new("emei.draft"))
            .label(tl!("Draft").to_string())
            .icon(IconId::new("draft"))
            .selected(emei::is_draft(&app.session))
            .show(ui);
    });

    row.group(RibbonGroup::new("emei.level", tl!("Level").to_string(), IconId::new("outlinePromote").into()), |ui| {
        Stack::new().gap(Gap::Tight).show(ui, |ui, _| {
            Button::command(CommandRef::new("emei.promote")).label(tl!("Promote").to_string()).icon(IconId::new("outlinePromote")).show(ui);
            Button::command(CommandRef::new("emei.demote")).label(tl!("Demote").to_string()).icon(IconId::new("outlineDemote")).show(ui);
        });
    });
}
