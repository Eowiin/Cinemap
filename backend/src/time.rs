use chrono::{NaiveDate, Utc};
use chrono_tz::Europe::Paris;

pub const DATE_FORMAT: &str = "%Y-%m-%d";

pub fn paris_today() -> NaiveDate {
    Utc::now().with_timezone(&Paris).date_naive()
}
