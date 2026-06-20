mod client;
mod map;
mod query;
mod rank;
mod station;

pub use query::{has_unknown_prefix, prefix_examples_inline, SearchField, StationSearchQuery};
pub use rank::{explain_station_match, rank_explanation_label};
pub use station::{
    clean_tag_values, fallback_stations, normalize_codec, normalize_country_code,
    normalize_station_uuid, sanitize_bitrate, station_identity_matches, station_url_matches, Station,
    StationHealth, StationIdentity,
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
/// advanced search parameters. Friendly aliases such as `genre:`, `cc:`,
/// `language:`, `format:`, and `station:` are accepted too.
pub async fn search_stations(raw_query: &str) -> anyhow::Result<Vec<Station>> {
    let query = StationSearchQuery::parse(raw_query);
    if query.is_short() {
        return Ok(Vec::new());
    }

    let stations = search_query_with_mirrors(&query).await?;

    Ok(rank::rank_search_results(&query, stations))
}

pub async fn lookup_station_metadata(station: &Station) -> anyhow::Result<Option<Station>> {
    let query = StationSearchQuery::parse(&format!("name:{}", station.name));
    if query.is_short() {
        return Ok(None);
    }

    let candidates = rank::rank_search_results(&query, search_query_with_mirrors(&query).await?);
    Ok(select_metadata_match(station, candidates))
}

async fn search_query_with_mirrors(query: &StationSearchQuery) -> anyhow::Result<Vec<Station>> {
    let client = client::radio_browser_client()?;
    let https_result =
        client::search_stations_with_servers(&client, RADIO_BROWSER_HTTPS_SERVERS, query).await;

    match https_result {
        Ok(stations) => Ok(stations),
        Err(https_error) => {
            match client::search_stations_with_servers(&client, RADIO_BROWSER_HTTP_SERVERS, query)
                .await
            {
                Ok(stations) => Ok(stations),
                Err(http_error) => anyhow::bail!(
                    "Search temporarily unavailable. Tried {} Radio Browser mirrors. Details: HTTPS search failed: {https_error}; HTTP fallback failed: {http_error}",
                    RADIO_BROWSER_HTTPS_SERVERS.len() + RADIO_BROWSER_HTTP_SERVERS.len()
                ),
            }
        }
    }
}

fn select_metadata_match(station: &Station, candidates: Vec<Station>) -> Option<Station> {
    candidates
        .into_iter()
        .find(|candidate| station_identity_matches(station, candidate))
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

    #[test]
    fn metadata_match_prefers_uuid_identity() {
        let mut saved = Station::basic("Saved", "http://old", "Synthwave", "US", 0);
        saved.station_uuid = Some("UUID-1".to_string());
        let mut candidate = Station::basic("Saved Rich", "http://new", "Synthwave", "US", 128);
        candidate.station_uuid = Some("uuid-1".to_string());
        candidate.codec = "MP3".to_string();

        let matched = select_metadata_match(&saved, vec![candidate.clone()]);

        assert_eq!(matched, Some(candidate));
    }

    #[test]
    fn metadata_match_falls_back_to_normalized_url() {
        let saved = Station::basic("Saved", " HTTP://example.com/stream/ ", "Synthwave", "US", 0);
        let mut candidate = Station::basic("Saved Rich", "http://example.com/stream", "Synthwave", "US", 128);
        candidate.codec = "MP3".to_string();

        let matched = select_metadata_match(&saved, vec![candidate.clone()]);

        assert_eq!(matched, Some(candidate));
    }

    #[test]
    fn metadata_match_rejects_name_only_candidates() {
        let saved = Station::basic("Same Name", "http://saved", "Synthwave", "US", 0);
        let candidate = Station::basic("Same Name", "http://other", "Synthwave", "US", 128);

        assert_eq!(select_metadata_match(&saved, vec![candidate]), None);
    }
}
