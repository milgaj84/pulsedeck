use super::map::{map_api_station, ApiBrowseStation};
use super::query::StationSearchQuery;
use super::Station;

pub(super) fn radio_browser_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("PulseDeck/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()?)
}

pub(super) async fn search_stations_with_servers(
    client: &reqwest::Client,
    servers: &[&str],
    query: &StationSearchQuery,
) -> anyhow::Result<Vec<Station>> {
    let mut errors = Vec::new();

    for server in servers {
        let url = radio_browser_search_url(server);
        match search_stations_on_server(client, &url, query).await {
            Ok(stations) => return Ok(stations),
            Err(err) => errors.push(format!("{server}: {err}")),
        }
    }

    anyhow::bail!("{}", format_search_errors(&errors))
}

fn radio_browser_search_url(server: &str) -> String {
    format!("{server}/json/stations/search")
}

fn format_search_errors(errors: &[String]) -> String {
    errors.join(" | ")
}

async fn search_stations_on_server(
    client: &reqwest::Client,
    url: &str,
    query: &StationSearchQuery,
) -> anyhow::Result<Vec<Station>> {
    let params = query.api_params();
    let resp = client
        .get(url)
        .query(&params)
        .send()
        .await?
        .error_for_status()?;

    let api_stations = resp.json::<Vec<ApiBrowseStation>>().await?;

    Ok(api_stations
        .into_iter()
        .filter_map(map_api_station)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radio::{RADIO_BROWSER_HTTPS_SERVERS, RADIO_BROWSER_HTTP_SERVERS};

    #[test]
    fn radio_browser_search_url_appends_expected_path() {
        assert_eq!(
            radio_browser_search_url("https://de1.api.radio-browser.info"),
            "https://de1.api.radio-browser.info/json/stations/search"
        );
    }

    #[test]
    fn https_servers_are_tried_before_http_fallback_servers() {
        assert!(RADIO_BROWSER_HTTPS_SERVERS
            .iter()
            .all(|server| server.starts_with("https://")));
        assert!(RADIO_BROWSER_HTTP_SERVERS
            .iter()
            .all(|server| server.starts_with("http://")));
        assert_eq!(
            RADIO_BROWSER_HTTPS_SERVERS.len(),
            RADIO_BROWSER_HTTP_SERVERS.len()
        );
    }

    #[test]
    fn format_search_errors_keeps_server_context() {
        let errors = vec![
            "https://de1.api.radio-browser.info: timeout".to_string(),
            "https://de2.api.radio-browser.info: tls".to_string(),
        ];

        assert_eq!(
            format_search_errors(&errors),
            "https://de1.api.radio-browser.info: timeout | https://de2.api.radio-browser.info: tls"
        );
    }
}
