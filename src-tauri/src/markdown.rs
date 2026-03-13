use comrak::options::Plugins;
use comrak::plugins::syntect::SyntectAdapterBuilder;
use comrak::{markdown_to_html_with_plugins, Options};
use serde::Serialize;
use std::io::Cursor;
use syntect::highlighting::ThemeSet;

const DEFAULT_MARKDOWN_THEME: &str = "zenburn";
const MARKDOWN_THEMES: [(&str, &[u8]); 9] = [
    ("zenburn", include_bytes!("themes/zenburn.tmTheme")),
    ("tokyo-night", include_bytes!("themes/tokyo-night.tmTheme")),
    ("everforest-dark", include_bytes!("themes/everforest-dark.tmTheme")),
    ("rose-pine", include_bytes!("themes/rose-pine.tmTheme")),
    ("rose-pine-moon", include_bytes!("themes/rose-pine-moon.tmTheme")),
    ("kanagawa-wave", include_bytes!("themes/kanagawa-wave.tmTheme")),
    ("kanagawa-dragon", include_bytes!("themes/kanagawa-dragon.tmTheme")),
    ("vitesse-dark", include_bytes!("themes/vitesse-dark.tmTheme")),
    ("vitesse-black", include_bytes!("themes/vitesse-black.tmTheme")),
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRenderResult {
    pub html: String,
    pub theme: String,
}

fn markdown_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.tagfilter = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.inline_footnotes = true;
    options.extension.description_lists = true;
    options.extension.multiline_block_quotes = true;
    options.extension.alerts = true;
    options.extension.header_ids = Some(String::new());
    options.render.r#unsafe = false;
    options
}

fn load_markdown_theme_set() -> Result<ThemeSet, String> {
    let mut theme_set = ThemeSet::new();

    for (name, bytes) in MARKDOWN_THEMES {
        let mut cursor = Cursor::new(bytes);
        let theme = ThemeSet::load_from_reader(&mut cursor)
            .map_err(|error| format!("failed to load markdown theme '{name}': {error}"))?;
        theme_set.themes.insert(name.to_string(), theme);
    }

    Ok(theme_set)
}

fn resolve_markdown_theme(requested: Option<&str>) -> &'static str {
    let requested = requested.map(str::trim).filter(|theme| !theme.is_empty());

    if let Some(theme) = requested {
        if let Some((resolved, _)) = MARKDOWN_THEMES.iter().find(|(candidate, _)| *candidate == theme) {
            return resolved;
        }
    }

    DEFAULT_MARKDOWN_THEME
}

fn render_markdown_with_theme(content: &str, requested_theme: Option<&str>) -> Result<MarkdownRenderResult, String> {
    let resolved_theme = resolve_markdown_theme(requested_theme);
    let adapter = SyntectAdapterBuilder::new()
        .theme(resolved_theme)
        .theme_set(load_markdown_theme_set()?)
        .build();

    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&adapter);

    let html = markdown_to_html_with_plugins(content, &markdown_options(), &plugins);
    Ok(MarkdownRenderResult { html, theme: resolved_theme.to_string() })
}

#[tauri::command]
pub fn list_markdown_themes() -> Vec<String> {
    MARKDOWN_THEMES.iter().map(|(name, _)| (*name).to_string()).collect()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn render_markdown(content: String, theme: Option<String>) -> Result<MarkdownRenderResult, String> {
    render_markdown_with_theme(&content, theme.as_deref())
}

#[cfg(test)]
mod tests {
    use super::{list_markdown_themes, render_markdown_with_theme};

    #[test]
    fn list_markdown_themes_includes_embedded_themes() {
        let themes = list_markdown_themes();
        assert!(themes.contains(&"zenburn".to_string()));
        assert!(themes.contains(&"tokyo-night".to_string()));
    }

    #[test]
    fn render_markdown_supports_gfm_extensions() {
        let rendered = render_markdown_with_theme(
            "# Heading\n\n~~gone~~\n\n- [x] done\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n",
            Some("zenburn"),
        )
        .expect("markdown should render");

        assert_eq!(rendered.theme, "zenburn");
        assert!(rendered.html.contains("<h1>"));
        assert!(rendered.html.contains("<del>gone</del>"));
        assert!(rendered.html.contains("type=\"checkbox\""));
        assert!(rendered.html.contains("<table>"));
    }

    #[test]
    fn render_markdown_uses_requested_theme_for_highlighting() {
        let rendered = render_markdown_with_theme("```rust\nfn main() {}\n```", Some("tokyo-night"))
            .expect("highlighted markdown should render");

        assert_eq!(rendered.theme, "tokyo-night");
        assert!(rendered.html.contains("language-rust"));
        assert!(rendered.html.contains("style=\""));
    }

    #[test]
    fn render_markdown_falls_back_to_default_theme() {
        let rendered = render_markdown_with_theme("`inline`", Some("missing-theme"))
            .expect("markdown should render with fallback theme");

        assert_eq!(rendered.theme, "zenburn");
    }

    #[test]
    fn render_markdown_strips_raw_html() {
        let rendered = render_markdown_with_theme("before<script>alert(1)</script>after", Some("zenburn"))
            .expect("markdown should render");

        assert!(!rendered.html.contains("<script"));
        assert!(rendered.html.contains("before"));
        assert!(rendered.html.contains("after"));
    }
}
