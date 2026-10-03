use std::time::Duration;

use reqwest::{
    Client, Error,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};

fn build_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
            (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        ),
    );
    headers
}

pub fn build_client() -> Result<Client, Error> {
    Client::builder()
        .default_headers(build_headers())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .retry(reqwest::retry::never())
        .build()
}
