use super::{RadioApi, Station};

/// Production implementation of [`RadioApi`] backed by the Radio Browser public API.
///
/// Stateless — a new `reqwest::Client` is created per search via the existing
/// mirror-fallback logic in the parent module.
pub struct RadioBrowserApi;

impl RadioApi for RadioBrowserApi {
    async fn search_stations(&self, query: &str) -> Result<Vec<Station>, String> {
        super::search_stations(query)
            .await
            .map_err(|err| err.to_string())
    }

    async fn discover_stations(&self, tag: &str) -> Result<Vec<Station>, String> {
        let query = format!("tag:{tag}");
        self.search_stations(&query).await
    }
}
