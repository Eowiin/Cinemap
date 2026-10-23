//! Choix du film TMDB qui correspond à un film AlloCiné (fonctions pures).

use super::api::{MovieDetails, Provider, RegionProviders, SearchResult, Video};
use crate::api::types::{WatchProvider, WatchProviders};
use crate::text;

const BACKDROP_BASE_URL: &str = "https://image.tmdb.org/t/p/w1280";
const LOGO_BASE_URL: &str = "https://image.tmdb.org/t/p/w92";
/// En dessous, la moyenne TMDB ne veut pas dire grand-chose.
const MIN_VOTES_FOR_RATING: i64 = 10;
/// L'année de production AlloCiné et l'année de sortie TMDB diffèrent souvent d'un an
/// (tournage en fin d'année, sortie en festival puis en salles).
const YEAR_TOLERANCE: i64 = 1;

/// Titre comparable : minuscules, sans accents, lettres et chiffres seulement
/// (« Mission : Impossible » = « Mission: Impossible »).
pub fn comparable_title(title: &str) -> String {
    text::normalize(title)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Premier résultat (TMDB trie par pertinence) dont le titre français ou original est
/// exactement l'un de `titles`, et dont l'année est compatible avec `year` si les deux
/// sont connues. Pas de similarité approximative : mieux vaut pas de bande-annonce
/// que celle d'un autre film.
pub fn best_match<'a>(
    candidates: &'a [SearchResult],
    titles: &[&str],
    year: Option<i64>,
) -> Option<&'a SearchResult> {
    let wanted: Vec<String> = titles.iter().map(|t| comparable_title(t)).collect();
    candidates.iter().find(|candidate| {
        let title_ok = [&candidate.title, &candidate.original_title]
            .iter()
            .any(|t| wanted.contains(&comparable_title(t)));
        let year_ok = match (year, candidate.release_year()) {
            (Some(year), Some(candidate_year)) => (year - candidate_year).abs() <= YEAR_TOLERANCE,
            _ => true,
        };
        title_ok && year_ok
    })
}

/// Ce que l'on enregistre en base à partir d'une fiche TMDB.
#[derive(Debug, PartialEq)]
pub struct Enrichment {
    pub tmdb_id: i64,
    pub backdrop_url: Option<String>,
    pub trailer_url: Option<String>,
    pub rating: Option<f64>,
    pub watch_providers: WatchProviders,
}

pub fn enrichment(details: &MovieDetails) -> Enrichment {
    Enrichment {
        tmdb_id: details.id,
        backdrop_url: details
            .backdrop_path
            .as_deref()
            .filter(|path| !path.is_empty())
            .map(|path| format!("{BACKDROP_BASE_URL}{path}")),
        trailer_url: best_trailer(&details.videos.results)
            .map(|video| format!("https://www.youtube.com/watch?v={}", video.key)),
        rating: (details.vote_count >= MIN_VOTES_FOR_RATING)
            .then(|| (details.vote_average * 10.0).round() / 10.0),
        watch_providers: details
            .watch_providers
            .results
            .fr
            .as_ref()
            .map(watch_providers)
            .unwrap_or_default(),
    }
}

/// Offres françaises, sans les doublons de JustWatch : les chaînes revendues par une
/// autre plateforme (« HBO Max Amazon Channel ») et les formules d'une plateforme déjà
/// listée (« Netflix Standard with Ads », « Paramount Plus Premium »).
fn watch_providers(region: &RegionProviders) -> WatchProviders {
    WatchProviders {
        link: region.link.clone(),
        free: providers([&region.free, &region.ads]),
        flatrate: providers([&region.flatrate]),
        rent: providers([&region.rent]),
        buy: providers([&region.buy]),
    }
}

fn providers<const N: usize>(lists: [&Vec<Provider>; N]) -> Vec<WatchProvider> {
    let mut kept: Vec<&Provider> = lists
        .into_iter()
        .flatten()
        .filter(|p| !p.provider_name.ends_with(" Channel"))
        .collect();
    kept.sort_by_key(|p| p.display_priority);
    // Une dizaine d'offres au plus : la double boucle ne coûte rien.
    let is_plan_of_another = |p: &Provider| {
        kept.iter().any(|base| {
            p.provider_name
                .strip_prefix(base.provider_name.as_str())
                .is_some_and(|rest| rest.starts_with(' '))
        })
    };
    let mut out: Vec<WatchProvider> = Vec::with_capacity(kept.len());
    for &p in &kept {
        if !is_plan_of_another(p) && out.iter().all(|seen| seen.name != p.provider_name) {
            out.push(WatchProvider {
                name: p.provider_name.clone(),
                logo_url: p
                    .logo_path
                    .as_ref()
                    .map(|path| format!("{LOGO_BASE_URL}{path}")),
            });
        }
    }
    out
}

/// Une vidéo YouTube de type bande-annonce (à défaut, teaser), en préférant le français
/// puis les vidéos officielles.
fn best_trailer(videos: &[Video]) -> Option<&Video> {
    videos
        .iter()
        .filter(|v| v.site == "YouTube" && (v.kind == "Trailer" || v.kind == "Teaser"))
        .min_by_key(|v| {
            (
                v.kind != "Trailer",
                v.iso_639_1.as_deref() != Some("fr"),
                !v.official,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmdb::api::Videos;

    fn provider(name: &str, priority: i64) -> Provider {
        Provider {
            provider_name: name.into(),
            logo_path: Some(format!("/{priority}.png")),
            display_priority: priority,
        }
    }

    fn candidate(id: i64, title: &str, original_title: &str, date: &str) -> SearchResult {
        SearchResult {
            id,
            title: title.into(),
            original_title: original_title.into(),
            release_date: Some(date.into()),
        }
    }

    fn video(key: &str, kind: &str, lang: &str, official: bool) -> Video {
        Video {
            key: key.into(),
            site: "YouTube".into(),
            kind: kind.into(),
            official,
            iso_639_1: Some(lang.into()),
        }
    }

    #[test]
    fn comparable_title_ignores_case_accents_and_punctuation() {
        assert_eq!(
            comparable_title("Mission : Impossible"),
            comparable_title("mission: impossible")
        );
        assert_eq!(comparable_title("L'Été dernier"), "letedernier");
    }

    #[test]
    fn matches_original_title_with_year_tolerance() {
        let candidates = [
            candidate(1, "Stagecoach", "Stagecoach", "1966-06-15"),
            candidate(2, "La Chevauchée fantastique", "Stagecoach", "1939-03-02"),
        ];
        let found = best_match(
            &candidates,
            &["Stagecoach", "La Chevauchée fantastique"],
            Some(1939),
        );
        assert_eq!(found.map(|c| c.id), Some(2));
        let found = best_match(&candidates, &["Stagecoach"], Some(1940));
        assert_eq!(found.map(|c| c.id), Some(2));
    }

    #[test]
    fn matches_french_title_when_original_differs() {
        let candidates = [candidate(7, "Malfaisante", "Other Mommy", "2026-10-10")];
        let found = best_match(&candidates, &["Malfaisante"], Some(2026));
        assert_eq!(found.map(|c| c.id), Some(7));
    }

    #[test]
    fn rejects_wrong_year_or_similar_title() {
        let candidates = [
            candidate(1, "Django", "Django", "1966-04-06"),
            candidate(2, "Django Unchained", "Django Unchained", "2012-12-25"),
        ];
        assert!(best_match(&candidates, &["Django"], Some(2016)).is_none());
    }

    #[test]
    fn unknown_dates_do_not_block_a_title_match() {
        let candidates = [SearchResult {
            id: 3,
            title: "Moscas".into(),
            original_title: "Moscas".into(),
            release_date: Some(String::new()),
        }];
        assert_eq!(
            best_match(&candidates, &["Moscas"], Some(2026)).map(|c| c.id),
            Some(3)
        );
    }

    #[test]
    fn enrichment_prefers_french_official_trailer() {
        let details = MovieDetails {
            id: 42,
            backdrop_path: Some("/b.jpg".into()),
            vote_average: 7.26,
            vote_count: 500,
            videos: Videos {
                results: vec![
                    video("teaser", "Teaser", "fr", true),
                    video("en", "Trailer", "en", true),
                    video("fr-fan", "Trailer", "fr", false),
                    video("fr", "Trailer", "fr", true),
                    video("clip", "Clip", "fr", true),
                ],
            },
            watch_providers: Default::default(),
        };
        assert_eq!(
            enrichment(&details),
            Enrichment {
                tmdb_id: 42,
                backdrop_url: Some("https://image.tmdb.org/t/p/w1280/b.jpg".into()),
                trailer_url: Some("https://www.youtube.com/watch?v=fr".into()),
                rating: Some(7.3),
                watch_providers: WatchProviders::default(),
            }
        );
    }

    #[test]
    fn enrichment_without_votes_or_videos_is_mostly_null() {
        let details = MovieDetails {
            id: 1,
            backdrop_path: None,
            vote_average: 9.0,
            vote_count: 3,
            videos: Videos::default(),
            watch_providers: Default::default(),
        };
        let e = enrichment(&details);
        assert_eq!(
            (e.backdrop_url, e.trailer_url, e.rating),
            (None, None, None)
        );
    }

    #[test]
    fn watch_providers_drop_duplicates_and_merge_free_offers() {
        let region = RegionProviders {
            link: Some("https://www.themoviedb.org/movie/1/watch?locale=FR".into()),
            flatrate: vec![
                provider("HBO Max Amazon Channel", 3),
                provider("Netflix Standard with Ads", 2),
                provider("HBO Max", 4),
                provider("Paramount Plus Premium", 5),
                provider("Paramount Plus", 6),
                provider("Netflix", 1),
            ],
            free: vec![provider("arte.tv", 9)],
            ads: vec![provider("Pluto TV", 7), provider("arte.tv", 9)],
            rent: vec![provider("Apple TV Store", 5)],
            buy: Vec::new(),
        };
        let w = watch_providers(&region);
        let names =
            |list: &[WatchProvider]| list.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&w.flatrate), ["Netflix", "HBO Max", "Paramount Plus"]);
        assert_eq!(names(&w.free), ["Pluto TV", "arte.tv"]);
        assert_eq!(names(&w.rent), ["Apple TV Store"]);
        assert!(w.buy.is_empty());
        assert_eq!(
            w.rent[0].logo_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w92/5.png")
        );
    }
}
