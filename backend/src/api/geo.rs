const EARTH_RADIUS_KM: f64 = 6371.0;
const KM_PER_LAT_DEGREE: f64 = 111.32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub lat: f64,
    pub lng: f64,
}

/// Rectangle qui contient le cercle de recherche : filtre grossier en SQL
/// (`lat BETWEEN … AND lng BETWEEN …`) avant le calcul exact en Rust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub max_lat: f64,
    pub min_lng: f64,
    pub max_lng: f64,
}

pub fn haversine_km(a: Position, b: Position) -> f64 {
    let dlat = (b.lat - a.lat).to_radians();
    let dlng = (b.lng - a.lng).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.lat.to_radians().cos() * b.lat.to_radians().cos() * (dlng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

pub fn bounding_box(center: Position, radius_km: f64) -> BoundingBox {
    let dlat = radius_km / KM_PER_LAT_DEGREE;
    // Un degré de longitude rétrécit vers les pôles.
    let dlng = radius_km / (KM_PER_LAT_DEGREE * center.lat.to_radians().cos());
    BoundingBox {
        min_lat: center.lat - dlat,
        max_lat: center.lat + dlat,
        min_lng: center.lng - dlng,
        max_lng: center.lng + dlng,
    }
}

/// Arrondi à 0,1 km : la précision des coordonnées ne justifie pas plus.
pub fn round_km(d: f64) -> f64 {
    (d * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARIS: Position = Position {
        lat: 48.8566,
        lng: 2.3522,
    };
    const LYON: Position = Position {
        lat: 45.7640,
        lng: 4.8357,
    };

    fn contains(bbox: BoundingBox, p: Position) -> bool {
        (bbox.min_lat..=bbox.max_lat).contains(&p.lat)
            && (bbox.min_lng..=bbox.max_lng).contains(&p.lng)
    }

    #[test]
    fn same_point_is_zero_km() {
        assert_eq!(haversine_km(PARIS, PARIS), 0.0);
    }

    #[test]
    fn paris_lyon_is_about_392_km() {
        let d = haversine_km(PARIS, LYON);
        assert!((d - 392.0).abs() < 2.0, "{d}");
    }

    #[test]
    fn bounding_box_keeps_14_km_and_drops_16_km() {
        let bbox = bounding_box(PARIS, 15.0);
        let north = |km: f64| Position {
            lat: PARIS.lat + km / KM_PER_LAT_DEGREE,
            lng: PARIS.lng,
        };
        assert!(contains(bbox, north(14.0)));
        assert!(!contains(bbox, north(16.0)));
    }

    #[test]
    fn bounding_box_is_wider_in_longitude_away_from_equator() {
        let bbox = bounding_box(PARIS, 15.0);
        assert!(bbox.max_lng - bbox.min_lng > bbox.max_lat - bbox.min_lat);
    }

    #[test]
    fn round_km_keeps_one_decimal() {
        assert_eq!(round_km(1.24), 1.2);
        assert_eq!(round_km(1.25), 1.3);
        assert_eq!(round_km(0.0), 0.0);
    }
}
