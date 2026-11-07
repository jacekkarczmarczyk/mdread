use comrak::{Options, markdown_to_html};

pub fn render(source: &str) -> String {
    markdown_to_html(source, &options())
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    let ext = &mut options.extension;
    ext.strikethrough = true;
    ext.table = true;
    ext.autolink = true;
    ext.tasklist = true;
    ext.footnotes = true;
    ext.front_matter_delimiter = Some("---".into());
    ext.alerts = true;
    ext.math_dollars = true;
    ext.math_code = true;
    ext.header_id_prefix = Some(String::new());
    options.render.github_pre_lang = true;
    options.render.sourcepos = true;
    options
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn renders_gfm() {
        let html = render("| a |\n|---|\n| b |\n\n- [x] done\n\n~~gone~~");
        assert!(html.contains("<table"));
        assert!(html.contains("type=\"checkbox\""));
        assert!(html.contains("<del"));
    }

    #[test]
    fn strips_front_matter() {
        let html = render("---\ntitle: x\n---\n\n# Hi");
        assert!(!html.contains("title: x"));
        assert!(html.contains("<h1"));
    }

    #[test]
    fn escapes_raw_html() {
        let html = render("<script>alert(1)</script>");
        assert!(!html.contains("<script>"));
    }
}
