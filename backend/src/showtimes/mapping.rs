use super::allocine::Showtime;

pub(super) enum Version {
    Vf,
    Vo,
    Vost,
}

impl Version {
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::Vf => "VF",
            Self::Vo => "VO",
            Self::Vost => "VOST",
        }
    }
}

/// Règles (voir `SOURCES.md` §4, « Mapping de version ») :
/// 1. `Localization.Version.French` ou `Localization.Language.French` (`LOCAL`, film en
///    langue française, souvent avec sous-titres SME) → VF ;
/// 2. `Localization.Version.Original` : film dont la première langue est le français → VF,
///    sinon sous-titré (`Localization.Subtitle.French` ou `Showtime.Accessibility.Subtitled`,
///    vu dans les groupes `*_st`) → VOST, sinon VO ;
/// 3. aucun tag `Localization.*` : `diffusionVersion` `DUBBED` (doublé en français, vu sur
///    C0014 le 2026-10-08 pour un film norvégien) ou `LOCAL` (langue locale) → VF ;
/// 4. autre cas → `None` : l'appelant saute la séance plutôt que d'inventer une version.
pub(super) fn version(showtime: &Showtime, languages: &[Option<String>]) -> Option<Version> {
    let has_tag = |wanted: &str| showtime.tags.iter().any(|tag| tag == wanted);

    if has_tag("Localization.Version.French") || has_tag("Localization.Language.French") {
        return Some(Version::Vf);
    }

    if has_tag("Localization.Version.Original") {
        if languages.first().and_then(Option::as_deref) == Some("FRENCH") {
            return Some(Version::Vf);
        }
        if has_tag("Localization.Subtitle.French") || has_tag("Showtime.Accessibility.Subtitled") {
            return Some(Version::Vost);
        }
        return Some(Version::Vo);
    }

    if matches!(showtime.diffusion_version.as_str(), "DUBBED" | "LOCAL") {
        return Some(Version::Vf);
    }

    None
}

pub(super) fn formats(showtime: &Showtime) -> Vec<&'static str> {
    let mut formats = Vec::new();
    for value in showtime
        .experience
        .iter()
        .chain(showtime.projection.iter())
        .chain(showtime.picture.iter())
        .chain(showtime.sound.iter())
        .flatten()
        .chain(showtime.tags.iter())
    {
        let mapped = match value.as_str() {
            "3D" | "Format.Projection.3D" => Some("3D"),
            "IMAX" | "Format.Experience.IMAX" => Some("IMAX"),
            "4DX" | "Format.Experience.4DX" => Some("4DX"),
            "ScreenX" | "SCREENX" | "Format.Experience.ScreenX" => Some("ScreenX"),
            "Dolby Cinema" | "DOLBY_CINEMA" | "Format.Experience.DolbyCinema" => {
                Some("Dolby Cinema")
            }
            "Dolby Atmos" | "DOLBY_ATMOS" | "Format.Sound.DolbyAtmos" => Some("Dolby Atmos"),
            "DIGITAL" | "ANALOG" | "DOLBY_71" => None,
            _ => {
                tracing::debug!(value, "format AlloCiné non reconnu");
                None
            }
        };
        if let Some(format) = mapped
            && !formats.contains(&format)
        {
            formats.push(format);
        }
    }
    formats
}

// Prefer the cinema's desktop booking link; use the relay link only when no default link exists.
pub(super) fn booking_url(showtime: &Showtime) -> Option<&str> {
    showtime
        .data
        .ticketing
        .iter()
        .find(|ticket| ticket.provider == "default" && ticket.kind == "DESKTOP")
        .and_then(|ticket| ticket.urls.first())
        .or_else(|| {
            showtime
                .data
                .ticketing
                .iter()
                .find(|ticket| ticket.provider == "relay")
                .and_then(|ticket| ticket.urls.first())
        })
        .map(String::as_str)
}

pub(super) fn runtime_minutes(runtime: &str) -> Option<u32> {
    let (hours, minutes) = runtime.trim_end_matches("min").split_once('h')?;
    let duration = hours.trim().parse::<u32>().ok()? * 60 + minutes.trim().parse::<u32>().ok()?;
    (duration != 0).then_some(duration)
}

pub(super) fn full_name(first: Option<&str>, last: Option<&str>) -> Option<String> {
    match (first, last) {
        (Some(first), Some(last)) => Some(format!("{first} {last}")),
        (Some(first), None) => Some(first.to_owned()),
        (None, Some(last)) => Some(last.to_owned()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::showtimes::allocine::{ShowtimeData, Ticketing};

    fn showtime(tags: &[&str]) -> Showtime {
        Showtime {
            internal_id: 1,
            starts_at: "2026-10-06T20:15:00".into(),
            diffusion_version: "ORIGINAL".into(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            projection: None,
            sound: None,
            picture: None,
            experience: None,
            data: ShowtimeData {
                ticketing: Vec::new(),
            },
        }
    }

    fn languages(values: &[Option<&str>]) -> Vec<Option<String>> {
        values
            .iter()
            .map(|value| value.map(str::to_owned))
            .collect()
    }

    #[test]
    fn runtime_minutes_parses_hours_and_minutes() {
        assert_eq!(runtime_minutes("1h 55min"), Some(115));
    }

    #[test]
    fn runtime_minutes_returns_none_for_zero_duration() {
        assert_eq!(runtime_minutes("0h 00min"), None);
    }

    #[test]
    fn runtime_minutes_returns_none_for_empty_string() {
        assert_eq!(runtime_minutes(""), None);
    }

    #[test]
    fn runtime_minutes_returns_none_for_invalid_input() {
        assert_eq!(runtime_minutes("abc"), None);
    }

    #[test]
    fn full_name_joins_first_and_last_name() {
        assert_eq!(
            full_name(Some("Isabelle"), Some("Huppert")),
            Some("Isabelle Huppert".to_owned())
        );
    }

    #[test]
    fn full_name_returns_first_name_when_last_name_is_missing() {
        assert_eq!(
            full_name(Some("Isabelle"), None),
            Some("Isabelle".to_owned())
        );
    }

    #[test]
    fn full_name_returns_last_name_when_first_name_is_missing() {
        assert_eq!(full_name(None, Some("Huppert")), Some("Huppert".to_owned()));
    }

    #[test]
    fn full_name_returns_none_when_both_names_are_missing() {
        assert_eq!(full_name(None, None), None);
    }

    #[test]
    fn version_maps_french_localization_to_vf() {
        let result = version(
            &showtime(&["Localization.Version.French"]),
            &languages(&[Some("FRENCH")]),
        );
        assert!(matches!(&result, Some(Version::Vf)));
        assert_eq!(result.unwrap().as_str(), "VF");
    }

    #[test]
    fn version_keeps_french_subtitle_for_french_language_as_vf() {
        let result = version(
            &showtime(&[
                "Localization.Version.French",
                "Localization.Subtitle.French",
            ]),
            &languages(&[Some("FRENCH")]),
        );
        assert!(matches!(&result, Some(Version::Vf)));
    }

    #[test]
    fn version_maps_original_with_french_subtitles_to_vost() {
        let result = version(
            &showtime(&[
                "Localization.Version.Original",
                "Localization.Subtitle.French",
            ]),
            &languages(&[Some("ENGLISH"), Some("JAPANESE")]),
        );
        assert!(matches!(&result, Some(Version::Vost)));
        assert_eq!(result.unwrap().as_str(), "VOST");
    }

    #[test]
    fn version_maps_original_without_subtitles_to_vo() {
        let result = version(
            &showtime(&["Localization.Version.Original"]),
            &languages(&[Some("ENGLISH")]),
        );
        assert!(matches!(&result, Some(Version::Vo)));
        assert_eq!(result.unwrap().as_str(), "VO");
    }

    #[test]
    fn version_maps_original_french_language_to_vf() {
        let result = version(
            &showtime(&["Localization.Version.Original"]),
            &languages(&[Some("FRENCH")]),
        );
        assert!(matches!(&result, Some(Version::Vf)));
    }

    #[test]
    fn version_uses_first_language_for_coproductions() {
        let result = version(
            &showtime(&["Localization.Version.Original"]),
            &languages(&[Some("CANTONESE"), Some("FRENCH")]),
        );
        assert!(matches!(&result, Some(Version::Vo)));
    }

    #[test]
    fn version_maps_local_french_language_to_vf() {
        // Vu sur P0095 le 2026-10-08 : groupe `multiple_sme`, `diffusionVersion: LOCAL`.
        let result = version(
            &showtime(&[
                "Showtime.Accessibility.Subtitled",
                "Localization.Language.French",
            ]),
            &languages(&[Some("FRENCH")]),
        );
        assert!(matches!(&result, Some(Version::Vf)));
    }

    #[test]
    fn version_maps_original_with_accessibility_subtitles_to_vost() {
        // Vu sur P0095 le 2026-10-08 : groupe `original_st`, film turc.
        let result = version(
            &showtime(&[
                "Showtime.Accessibility.Subtitled",
                "Localization.Version.Original",
            ]),
            &languages(&[Some("TURKISH")]),
        );
        assert!(matches!(&result, Some(Version::Vost)));
    }

    #[test]
    fn version_falls_back_to_dubbed_diffusion_version_without_localization_tags() {
        let mut screening = showtime(&["Format.Projection.Digital"]);
        screening.diffusion_version = "DUBBED".into();
        let result = version(&screening, &languages(&[Some("NORWEGIAN")]));
        assert!(matches!(&result, Some(Version::Vf)));
    }

    #[test]
    fn version_returns_none_when_version_tag_is_unknown() {
        assert!(version(&showtime(&[]), &languages(&[Some("FRENCH")])).is_none());
    }

    #[test]
    fn formats_maps_supported_values_and_ignores_other_fixture_values() {
        let mut screening = showtime(&[]);
        screening.experience = Some(vec!["IMAX".into(), "4DX".into()]);
        screening.projection = Some(vec!["DIGITAL".into(), "3D".into()]);
        screening.sound = Some(vec!["DOLBY_71".into(), "DOLBY_ATMOS".into()]);
        assert_eq!(
            formats(&screening),
            vec!["IMAX", "4DX", "3D", "Dolby Atmos"]
        );
    }

    #[test]
    fn booking_url_prefers_default_desktop_over_relay() {
        let mut screening = showtime(&[]);
        screening.data.ticketing = vec![
            Ticketing {
                urls: vec!["https://relay.example/".into()],
                kind: "DESKTOP".into(),
                provider: "relay".into(),
            },
            Ticketing {
                urls: vec!["https://cinema.example/".into()],
                kind: "DESKTOP".into(),
                provider: "default".into(),
            },
        ];
        assert_eq!(booking_url(&screening), Some("https://cinema.example/"));
    }

    #[test]
    fn booking_url_returns_none_without_ticketing() {
        assert_eq!(booking_url(&showtime(&[])), None);
    }
}
