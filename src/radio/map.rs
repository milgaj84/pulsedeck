use serde::Deserialize;

use super::station::clean_tag_values;
use super::Station;

#[derive(Debug, Deserialize)]
pub(super) struct ApiBrowseStation {
    #[serde(default)]
    stationuuid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    url_resolved: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    countrycode: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    bitrate: u32,
    #[serde(default)]
    lastcheckok: Option<u8>,
    #[serde(default)]
    votes: Option<u32>,
    #[serde(default)]
    clickcount: Option<u32>,
}

pub(super) fn map_api_station(api: ApiBrowseStation) -> Option<Station> {
    let url = preferred_stream_url(&api);
    if url.is_empty() {
        return None;
    }

    let tags = split_csv_values(&api.tags);
    let genre = tags.first().cloned().unwrap_or_else(|| "Radio".to_string());

    Some(Station {
        name: fallback_trimmed(api.name, "Unnamed station"),
        url,
        genre,
        country: api.country.trim().to_string(),
        bitrate: api.bitrate,
        station_uuid: non_empty(api.stationuuid),
        country_code: api.countrycode.trim().to_ascii_uppercase(),
        tags,
        language: api.language.trim().to_string(),
        codec: api.codec.trim().to_ascii_uppercase(),
        homepage: api.homepage.trim().to_string(),
        last_check_ok: api.lastcheckok.map(|value| value == 1),
        votes: api.votes,
        click_count: api.clickcount,
    })
}

fn preferred_stream_url(api: &ApiBrowseStation) -> String {
    let resolved = api.url_resolved.trim();
    if !resolved.is_empty() {
        return resolved.to_string();
    }
    api.url.trim().to_string()
}

fn split_csv_values(value: &str) -> Vec<String> {
    clean_tag_values(value.split(',').map(str::to_string).collect())
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn fallback_trimmed(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_station(name: &str, resolved_url: &str, tags: &str) -> ApiBrowseStation {
        ApiBrowseStation {
            stationuuid: String::new(),
            name: name.to_string(),
            url: String::new(),
            url_resolved: resolved_url.to_string(),
            tags: tags.to_string(),
            country: "US".to_string(),
            countrycode: "us".to_string(),
            language: String::new(),
            codec: String::new(),
            homepage: String::new(),
            bitrate: 128,
            lastcheckok: None,
            votes: None,
            clickcount: None,
        }
    }

    #[test]
    fn map_api_station_trims_name_and_uses_first_tag() {
        let station = map_api_station(api_station(
            "  Lo-Fi Radio  ",
            "http://stream",
            "lofi,chill",
        ))
        .expect("station should map");

        assert_eq!(station.name, "Lo-Fi Radio");
        assert_eq!(station.url, "http://stream");
        assert_eq!(station.genre, "lofi");
        assert_eq!(station.tags, vec!["lofi".to_string(), "chill".to_string()]);
        assert_eq!(station.country, "US");
        assert_eq!(station.country_code, "US");
        assert_eq!(station.bitrate, 128);
    }

    #[test]
    fn map_api_station_drops_empty_urls_when_both_urls_are_missing() {
        assert!(map_api_station(api_station("Broken", "", "radio")).is_none());
    }

    #[test]
    fn map_api_station_falls_back_to_raw_url_when_resolved_url_is_empty() {
        let mut api = api_station("Fallback", "", "radio");
        api.url = " http://raw ".to_string();

        let station = map_api_station(api).expect("station should map");
        assert_eq!(station.url, "http://raw");
    }

    #[test]
    fn map_api_station_maps_uuid_country_language_codec_homepage_and_popularity() {
        let mut api = api_station("Meta", "http://stream", "jazz");
        api.stationuuid = " uuid-1 ".to_string();
        api.countrycode = "ba".to_string();
        api.language = " Bosnian ".to_string();
        api.codec = "mp3".to_string();
        api.homepage = " https://example.com ".to_string();
        api.lastcheckok = Some(1);
        api.votes = Some(42);
        api.clickcount = Some(1200);

        let station = map_api_station(api).expect("station should map");
        assert_eq!(station.station_uuid.as_deref(), Some("uuid-1"));
        assert_eq!(station.country_code, "BA");
        assert_eq!(station.language, "Bosnian");
        assert_eq!(station.codec, "MP3");
        assert_eq!(station.homepage, "https://example.com");
        assert_eq!(station.last_check_ok, Some(true));
        assert_eq!(station.votes, Some(42));
        assert_eq!(station.click_count, Some(1200));
    }

    #[test]
    fn map_api_station_converts_lastcheckok_zero_to_false() {
        let mut api = api_station("Meta", "http://stream", "jazz");
        api.lastcheckok = Some(0);

        let station = map_api_station(api).expect("station should map");
        assert_eq!(station.last_check_ok, Some(false));
    }

    #[test]
    fn split_csv_values_trims_and_drops_empty_parts() {
        assert_eq!(
            split_csv_values(" jazz, , pop ,,rock "),
            vec!["jazz".to_string(), "pop".to_string(), "rock".to_string()]
        );
    }
}
