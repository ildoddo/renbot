//! OMDb: database facoltativo. Per ora è prevista la sola verifica della chiave.

use super::{ProviderError, ProviderResult};
use crate::http::{Fetcher, HttpError, HttpRequest};

const BASE_URL: &str = "https://www.omdbapi.com/";
/// Titolo usato per la chiamata di prova.
const SAMPLE_ID: &str = "tt0111161";

/// Verifica la chiave con una chiamata reale, senza passare dalla cache.
pub async fn verify_key(fetcher: &Fetcher, api_key: &str) -> ProviderResult<()> {
    let request = HttpRequest::get(BASE_URL)
        .query("i", SAMPLE_ID)
        .secret_query("apikey", api_key);
    let response = fetcher.json(&request, false).await?;
    if response["Response"] == "True" {
        Ok(())
    } else {
        Err(ProviderError::Http(HttpError::Unauthorized))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testing::{fetcher, status, FakeClient};

    #[tokio::test]
    async fn verifica_della_chiave() {
        let client = FakeClient::new();
        client.route_json("omdbapi", json!({"Title": "x", "Response": "True"}));
        assert_eq!(verify_key(&fetcher(client.clone()), "K").await, Ok(()));
        let request = &client.requests()[0];
        assert_eq!(
            request.secret_query,
            [("apikey".to_string(), "K".to_string())]
        );

        let unauthorized = Err(ProviderError::Http(HttpError::Unauthorized));
        let client = FakeClient::new();
        client.route("omdbapi", vec![status(401)]);
        assert_eq!(verify_key(&fetcher(client), "K").await, unauthorized);

        let client = FakeClient::new();
        client.route_json(
            "omdbapi",
            json!({"Response": "False", "Error": "Invalid API key!"}),
        );
        assert_eq!(verify_key(&fetcher(client), "K").await, unauthorized);
    }
}
