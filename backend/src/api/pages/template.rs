use std::path::Path;

use anyhow::{Context, bail};

/// Début et fin du bloc remplacé dans le `<head>` (titre et description par défaut,
/// gardés tels quels par `vite dev`).
const HEAD_START: &str = "<!--seo-->";
const HEAD_END: &str = "<!--/seo-->";
/// Contenu servi dans `<div id="app">`, remplacé par l'app au démarrage du JS.
const BODY: &str = "<!--app-->";

/// `index.html` du front découpé une fois au démarrage : une page = trois copies et
/// deux insertions, sans moteur de gabarits.
#[derive(Debug)]
pub struct PageTemplate {
    before_head: String,
    before_body: String,
    after_body: String,
}

impl PageTemplate {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let html = std::fs::read_to_string(path)
            .with_context(|| format!("lecture de {}", path.display()))?;
        Self::parse(&html).with_context(|| format!("gabarit {}", path.display()))
    }

    pub fn parse(html: &str) -> anyhow::Result<Self> {
        let (before_head, rest) = html
            .split_once(HEAD_START)
            .with_context(|| format!("{HEAD_START} absent"))?;
        let (_, rest) = rest
            .split_once(HEAD_END)
            .with_context(|| format!("{HEAD_END} absent"))?;
        let (before_body, after_body) = rest
            .split_once(BODY)
            .with_context(|| format!("{BODY} absent"))?;
        if after_body.contains(BODY) || after_body.contains(HEAD_START) {
            bail!("marqueur en double");
        }
        Ok(PageTemplate {
            before_head: before_head.to_owned(),
            before_body: before_body.to_owned(),
            after_body: after_body.to_owned(),
        })
    }

    pub fn render(&self, head: &str, body: &str) -> String {
        [
            self.before_head.as_str(),
            head,
            &self.before_body,
            body,
            &self.after_body,
        ]
        .concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = "<head><!--seo--><title>Cinemap</title><!--/seo--><link></head>\
                        <body><div id=\"app\"><!--app--></div></body>";

    #[test]
    fn replaces_the_head_block_and_fills_the_app() {
        let page = PageTemplate::parse(HTML)
            .unwrap()
            .render("<title>X</title>", "<h1>X</h1>");
        assert_eq!(
            page,
            "<head><title>X</title><link></head><body><div id=\"app\"><h1>X</h1></div></body>"
        );
    }

    #[test]
    fn refuses_a_template_without_markers() {
        assert!(PageTemplate::parse("<head></head><div id=\"app\"></div>").is_err());
        assert!(PageTemplate::parse("<!--seo--><!--/seo--><!--app--><!--app-->").is_err());
    }
}
