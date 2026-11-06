//! `{fields}` in title block texts: sheet, project and date values, and
//! the title block's per-sheet parameters. Unknown keys stay as written.

use cc_core::{ComponentDef, Project, View, ViewKind};

/// The fixed keys, in the order the editor's menu lists them.
pub const KEYS: [&str; 12] = [
    "sheet_number",
    "sheet_name",
    "sheet_count",
    "sheet_index",
    "sheet_scale",
    "paper",
    "project_name",
    "project_number",
    "client",
    "address",
    "author",
    "date",
];

/// What the fields read from.
pub struct FieldCtx<'a> {
    pub project: &'a Project,
    pub sheet: &'a View,
    /// The title block definition (for `param:` defaults), if any.
    pub def: Option<&'a ComponentDef>,
    pub today: (i32, u32, u32),
    pub format_date: fn(i32, u32, u32) -> String,
    /// "As indicated", translated.
    pub as_indicated: &'a str,
}

/// Most bytes a field key may have (a longer `{…}` is not a field).
const MAX_KEY: usize = 200;

/// Replaces each `{key}` of `text` with its value; `{{` and `}}` are literal
/// braces and unknown keys stay as written.
pub fn expand(text: &str, ctx: &FieldCtx) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // `find` lands on ASCII braces, so every slice below is on a char boundary.
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        if let Some(after) = tail.strip_prefix('}') {
            out.push('}');
            rest = after;
            continue;
        }
        match tail[1..].find('}').filter(|&j| j <= MAX_KEY) {
            Some(j) => {
                let key = &tail[1..1 + j];
                match value(key, ctx) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&tail[..j + 2]),
                }
                rest = &tail[j + 2..];
            }
            None => {
                out.push_str(tail);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The value of one field, or `None` for an unknown key.
pub fn value(key: &str, ctx: &FieldCtx) -> Option<String> {
    let ViewKind::Sheet { number, size, viewports, fields, issue_date, .. } = &ctx.sheet.kind else {
        return None;
    };
    let p = ctx.project;
    let info = &p.info;
    let date = |(y, m, d): (i32, u32, u32)| (ctx.format_date)(y, m, d);
    let v = match key.trim() {
        "sheet_number" => number.clone(),
        "sheet_name" => ctx.sheet.name.clone(),
        "sheet_count" => p.sheets_in_order().len().to_string(),
        "sheet_index" => {
            p.sheets_in_order().iter().position(|id| *id == ctx.sheet.id).map_or(String::new(), |i| (i + 1).to_string())
        }
        "sheet_scale" => sheet_scale(p, viewports, ctx.as_indicated),
        "paper" => size.label(),
        "project_name" => info.name.clone(),
        "project_number" => info.number.clone(),
        "client" => info.client.clone(),
        "address" => info.address.clone(),
        "author" => info.author.clone(),
        "date" => date(ctx.today),
        "issue_date" => cc_core::title_block::parse_iso_date(issue_date).map_or(String::new(), date),
        k => {
            if let Some(name) = k.strip_prefix("project:") {
                info.field(name.trim())?.to_string()
            } else {
                let name = k.strip_prefix("param:")?.trim();
                let param = ctx.def.and_then(|d| d.param(name)).filter(|q| q.kind.is_text());
                match (fields.get(name), param) {
                    (Some(v), Some(q)) if q.instance => v.clone(),
                    (_, Some(q)) => q.text.clone(),
                    (Some(v), None) => v.clone(),
                    (None, None) => return None,
                }
            }
        }
    };
    Some(v)
}

fn sheet_scale(p: &Project, viewports: &[cc_core::Viewport], as_indicated: &str) -> String {
    let mut scales = viewports
        .iter()
        .filter_map(|vp| p.view(vp.view))
        .filter(|v| !matches!(v.kind, ViewKind::ThreeD { .. }))
        .map(|v| v.scale);
    match scales.next() {
        None => String::new(),
        Some(first) if scales.all(|s| s == first) => format!("1:{first}"),
        Some(_) => as_indicated.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cc_core::{ComponentKind, ComponentParam, ParamKind, SheetSize, ViewId};

    fn iso(y: i32, m: u32, d: u32) -> String {
        format!("{y:04}-{m:02}-{d:02}")
    }

    fn sheet(id: u64, number: &str, fields: &[(&str, &str)], issue: &str) -> View {
        let kind = ViewKind::Sheet {
            number: number.into(),
            size: SheetSize::A1,
            portrait: false,
            viewports: Vec::new(),
            title_block: None,
            fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            issue_date: issue.into(),
        };
        View::new(ViewId(id), format!("Planta {number}"), 100, kind)
    }

    fn project() -> Project {
        let mut p = Project::from_template();
        p.views.retain(|_, v| !matches!(v.kind, ViewKind::Sheet { .. }));
        for v in [
            sheet(9001, "A10", &[], ""),
            sheet(9002, "A2", &[("Drawn", "Bia")], "2026-03-04"),
            sheet(9003, "A1", &[], ""),
        ] {
            p.views.insert(v.id, v);
        }
        p.info.name = "Casa".into();
        p.info.fields.push(cc_core::ProjectField { name: "CAU".into(), value: "A123".into() });
        p
    }

    fn def() -> ComponentDef {
        let mut def = ComponentDef { kind: ComponentKind::TitleBlock, ..Default::default() };
        let mut drawn = ComponentParam::number("Drawn", 0.0, ParamKind::Text);
        drawn.text = "Equipe".into();
        drawn.instance = true;
        def.params.push(drawn);
        def
    }

    #[test]
    fn fields_fill_in() {
        let p = project();
        let d = def();
        let ctx = |id: u64| FieldCtx {
            project: &p,
            sheet: p.view(ViewId(id)).unwrap(),
            def: Some(&d),
            today: (2026, 11, 6),
            format_date: iso,
            as_indicated: "Indicada",
        };
        let c = ctx(9002);
        assert_eq!(expand("Folha {sheet_number} ({sheet_index}/{sheet_count})", &c), "Folha A2 (2/3)");
        assert_eq!(expand("{project_name} · {project:CAU} · {paper}", &c), "Casa · A123 · A1");
        assert_eq!(expand("{date} {issue_date}", &c), "2026-11-06 2026-03-04");
        assert_eq!(expand("{param:Drawn}", &c), "Bia");
        assert_eq!(expand("{param:Drawn}", &ctx(9001)), "Equipe", "the type's default");
        assert_eq!(expand("[{issue_date}]", &ctx(9001)), "[]", "empty values vanish");
        assert_eq!(expand("{xyz} {sheet_number", &c), "{xyz} {sheet_number", "unknown stays");
        assert_eq!(expand("{{literal}}", &c), "{literal}");
        assert_eq!(expand("{sheet_scale}", &c), "", "no viewports");
    }

    #[test]
    fn multibyte_text_is_safe() {
        let p = project();
        let ctx = FieldCtx {
            project: &p,
            sheet: p.view(ViewId(9002)).unwrap(),
            def: None,
            today: (2026, 1, 1),
            format_date: iso,
            as_indicated: "x",
        };
        assert_eq!(expand("é{é}ü{sheet_number}ñ{", &ctx), "é{é}üA2ñ{");
    }

    #[test]
    fn scale_is_single_or_indicated() {
        let mut p = project();
        let plans: Vec<ViewId> = p.views.values().filter(|v| v.level().is_some()).map(|v| v.id).take(2).collect();
        assert_eq!(plans.len(), 2);
        for (i, id) in plans.iter().enumerate() {
            if let Some(v) = p.views.get_mut(id) {
                v.scale = if i == 0 { 50 } else { 100 };
            }
        }
        let place = |p: &mut Project, ids: &[ViewId]| {
            if let Some(ViewKind::Sheet { viewports, .. }) = p.views.get_mut(&ViewId(9001)).map(|v| &mut v.kind) {
                *viewports = ids.iter().map(|&view| cc_core::Viewport { view, center: glam::DVec2::ZERO }).collect();
            }
        };
        place(&mut p, &plans[..1]);
        {
            let ctx = FieldCtx {
                project: &p,
                sheet: p.view(ViewId(9001)).unwrap(),
                def: None,
                today: (2026, 1, 1),
                format_date: iso,
                as_indicated: "Indicada",
            };
            assert_eq!(expand("{sheet_scale}", &ctx), "1:50");
        }
        place(&mut p, &plans);
        let ctx = FieldCtx {
            project: &p,
            sheet: p.view(ViewId(9001)).unwrap(),
            def: None,
            today: (2026, 1, 1),
            format_date: iso,
            as_indicated: "Indicada",
        };
        assert_eq!(expand("{sheet_scale}", &ctx), "Indicada");
    }
}
