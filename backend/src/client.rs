use std::time::Duration;

use anyhow::Context;
use bytes::Bytes;
use reqwest::{
    Client, Error, RequestBuilder,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};
use tokio::time::sleep;
use tracing::warn;

/// Pauses entre deux essais pour une erreur passagère : 4 essais au total, ~1 min d'attente au pire.
pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(45),
];

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

/// Envoie la requête construite par `request`, avec un nouvel essai après chaque pause de
/// `delays` si l'erreur est passagère. `url` ne sert qu'aux messages.
///
/// `request` est une closure : un `RequestBuilder` est consommé par `send()`, il en faut
/// un neuf à chaque essai. L'appelant y ajoute ses en-têtes ou un délai propre à la source.
pub async fn fetch_with_retries(
    url: &str,
    delays: &[Duration],
    request: impl Fn() -> RequestBuilder,
) -> anyhow::Result<Bytes> {
    fetch_with_retries_and_hooks(url, delays, || async { Ok(()) }, |_| {}, request).await
}

/// Comme `fetch_with_retries`, avec deux crochets appelés à **chaque** essai (retries compris) :
/// `before_attempt` avant l'envoi (attendre le limiteur de débit, refuser si le coupe-circuit
/// est déclenché) et `observe_attempt` après la réponse (compter les 403/429).
pub async fn fetch_with_retries_and_hooks<B, Fut, O>(
    url: &str,
    delays: &[Duration],
    before_attempt: B,
    observe_attempt: O,
    request: impl Fn() -> RequestBuilder,
) -> anyhow::Result<Bytes>
where
    B: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
    O: Fn(&Result<Bytes, reqwest::Error>),
{
    let mut attempt = 0;
    loop {
        before_attempt()
            .await
            .with_context(|| format!("Préparation de la requête {url}"))?;
        let result = fetch_once(request()).await;
        observe_attempt(&result);
        match result {
            Ok(body) => return Ok(body),
            Err(error) => {
                if !is_retryable(&error) || attempt == delays.len() {
                    return Err(error).with_context(|| {
                        format!("Échec de {url} après {} tentative(s)", attempt + 1)
                    });
                }
                let delay = delays[attempt];
                warn!(
                    url,
                    error = %error,
                    next_attempt = attempt + 2,
                    delay_secs = delay.as_secs(),
                    "Erreur réseau passagère, nouvel essai prévu"
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

pub fn is_retryable(error: &reqwest::Error) -> bool {
    if let Some(status) = error.status() {
        return status.is_server_error();
    }

    if error.is_timeout() || error.is_connect() {
        return true;
    }

    // Une coupure peut être enveloppée dans une erreur de requête ou de
    // décodage. Seule sa cause d'E/S justifie alors un nouvel essai.
    let mut cause = std::error::Error::source(error);
    while let Some(source) = cause {
        if let Some(io_error) = source.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;
            return matches!(
                io_error.kind(),
                ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::TimedOut
            );
        }
        cause = source.source();
    }
    false
}

pub async fn fetch_once(request: RequestBuilder) -> Result<Bytes, reqwest::Error> {
    request.send().await?.error_for_status()?.bytes().await
}
