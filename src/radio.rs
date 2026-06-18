mod client;
mod map;
mod query;
mod rank;
mod station;

pub use query::StationSearchQuery;
pub use station::{
    clean_tag_values, fallback_stations, station_identity_matches, Station, StationIdentity,
};

const RADIO_BROWSER_HTTPS_SERVERS: &[&str] = &[
    "https://de1.api.radio-browser.info",
    "https://de2.api.radio-browser.info",
    "https://nl1.api.radio-browser.info",
    "https://at1.api.radio-browser.info",
];

const RADIO_BROWSER_HTTP_SERVERS: &[&str] = &[
    "http://de1.api.radio-browser.info",
    "http://de2.api.radio-browser.info",
    "http://nl1.api.radio-browser.info",
    "http://at1.api.radio-browser.info",
];

/// Search for stations through the Radio Browser API.
///
/// Plain text searches station names. Focused prefixes such as `tag:ambient`,
/// `country:BA`, `lang:english`, and `codec:mp3` map to Radio Browser's
/// advanced search parameters.
pub async fn search_stations(raw_query: &str) -> anyhow::Result<Vec<Station>> {
    let query = StationSearchQuery::parse(raw_query);
    if query.is_short() {
        return Ok(Vec::new());
    }

    let client = client::radio_browser_client()?;
    let https_result =
        client::search_stations_with_servers(&client, RADIO_BROWSER_HTTPS_SERVERS, &query).await;

    let stations = match https_result {
        Ok(stations) => stations,
        Err(https_error) => {
            match client::search_stations_with_servers(&client, RADIO_BROWSER_HTTP_SERVERS, &query)
                .await
            {
                Ok(stations) => stations,
                Err(http_error) => anyhow::bail!(
                    "HTTPS search failed: {https_error}; HTTP fallback failed: {http_error}"
                ),
            }
        }
    };

    Ok(rank::rank_search_results(&query, stations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_stations_include_known_offline_defaults() {
        let stations = fallback_stations();

        assert!(stations
            .iter()
            .any(|station| station.name == "Nightride FM"));
        assert!(stations
            .iter()
            .any(|station| station.name == "SomaFM: Groove Salad"));
        assert!(stations.iter().all(|station| !station.url.is_empty()));
        assert!(stations
            .iter()
            .all(|station| station.station_uuid.is_none()));
    }
}
