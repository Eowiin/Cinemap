use std::fmt::Write;

use chrono::NaiveDate;

pub const SITE_URL: &str = "https://cinemap.ethansaux.fr";
pub const SITE_NAME: &str = "Cinemap";

/// Texte sûr dans un nœud ou un attribut entre guillemets doubles.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Coupe à `max` caractères au plus, sur une fin de mot, avec « … ».
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    let cut = cut.rfind(' ').map_or(cut.as_str(), |i| &cut[..i]);
    format!("{}…", cut.trim_end_matches([' ', ',', '.', ';', ':']))
}

/// Ce qui change d'une page à l'autre dans le `<head>`.
pub struct Head<'a> {
    pub title: &'a str,
    pub description: &'a str,
    /// Chemin canonique (`/film/42-x`), sans query : les filtres ne font pas une autre page.
    pub path: &'a str,
    pub image: Option<&'a str>,
    pub og_type: &'static str,
    pub noindex: bool,
    pub json_ld: Option<serde_json::Value>,
}

impl Head<'_> {
    pub fn render(&self) -> String {
        let title = escape(self.title);
        let description = escape(self.description);
        let url = escape(&format!("{SITE_URL}{}", self.path));
        let mut head = format!(
            "<title>{title}</title>\n\
             <meta name=\"description\" content=\"{description}\" />\n\
             <meta property=\"og:site_name\" content=\"{SITE_NAME}\" />\n\
             <meta property=\"og:locale\" content=\"fr_FR\" />\n\
             <meta property=\"og:type\" content=\"{}\" />\n\
             <meta property=\"og:title\" content=\"{title}\" />\n\
             <meta property=\"og:description\" content=\"{description}\" />\n\
             <meta property=\"og:url\" content=\"{url}\" />\n",
            self.og_type
        );
        if let Some(image) = self.image {
            let _ = writeln!(
                head,
                "<meta property=\"og:image\" content=\"{}\" />",
                escape(image)
            );
        }
        // Une page exclue de l'index n'a pas d'URL canonique à annoncer.
        if self.noindex {
            head.push_str("<meta name=\"robots\" content=\"noindex\" />\n");
        } else {
            let _ = writeln!(head, "<link rel=\"canonical\" href=\"{url}\" />");
        }
        if let Some(json_ld) = &self.json_ld {
            let _ = writeln!(
                head,
                "<script type=\"application/ld+json\">{}</script>",
                script_json(json_ld)
            );
        }
        head
    }
}

/// JSON sûr dans un `<script>` : un titre contenant `</script>` ne doit pas le fermer.
fn script_json(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

const WEEKDAYS: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];
const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// « lundi 12 octobre » (« 1er » le premier du mois).
pub fn french_day(date: NaiveDate) -> String {
    use chrono::Datelike;
    let day = match date.day() {
        1 => "1er".to_owned(),
        n => n.to_string(),
    };
    format!(
        "{} {day} {}",
        WEEKDAYS[date.weekday().num_days_from_monday() as usize],
        MONTHS[date.month0() as usize]
    )
}

/// « 23 septembre 2026 » pour une date `YYYY-MM-DD`, la chaîne telle quelle sinon.
pub fn french_date(date: &str) -> String {
    use chrono::Datelike;
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year()),
        Err(_) => date.to_owned(),
    }
}

/// « 1 h 55 », comme le front (`formatRuntime`).
pub fn runtime(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, m) => format!("{h} h {m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_neutralises_markup() {
        assert_eq!(
            escape(r#"<b>"Tom & Jerry's"</b>"#),
            "&lt;b&gt;&quot;Tom &amp; Jerry&#39;s&quot;&lt;/b&gt;"
        );
    }

    #[test]
    fn json_ld_cannot_close_its_script() {
        let head = Head {
            title: "</script><script>alert(1)</script>",
            description: "d",
            path: "/",
            image: None,
            og_type: "website",
            noindex: false,
            json_ld: Some(serde_json::json!({ "name": "</script><script>alert(1)</script>" })),
        }
        .render();
        assert_eq!(head.matches("<script").count(), 1, "{head}");
        assert_eq!(head.matches("</script>").count(), 1, "{head}");
        assert!(head.contains("<title>&lt;/script&gt;"));
    }

    #[test]
    fn truncate_cuts_on_a_word() {
        assert_eq!(truncate("court", 10), "court");
        assert_eq!(truncate("Un film très, très long", 16), "Un film très…");
    }

    #[test]
    fn french_dates() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
        assert_eq!(french_day(date), "lundi 12 octobre");
        assert_eq!(
            french_day(NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()),
            "samedi 1er août"
        );
        assert_eq!(french_date("2026-09-23"), "23 septembre 2026");
        assert_eq!(runtime(115), "1 h 55");
        assert_eq!(runtime(120), "2 h 00");
        assert_eq!(runtime(45), "45 min");
    }
}
