use crate::radio::Station;

pub fn to_m3u(stations: &[Station]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for s in stations {
        if let Some(uuid) = s
            .station_uuid
            .as_deref()
            .map(str::trim)
            .filter(|uuid| !uuid.is_empty())
        {
            out.push_str(&format!("#RADIOBROWSERUUID:{}\n", uuid));
        }
        out.push_str(&format!("#EXTINF:-1,{}\n", s.name));
        out.push_str(&format!("#EXTGENRE:{}\n", s.genre));
        out.push_str(&format!("{}\n", s.url));
    }
    out
}

pub fn from_m3u(text: &str) -> Vec<Station> {
    let mut stations = Vec::new();
    let mut name = String::new();
    let mut genre = String::new();
    let mut station_uuid: Option<String> = None;

    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#RADIOBROWSERUUID:") {
            station_uuid = Some(rest.trim().to_string()).filter(|uuid| !uuid.is_empty());
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            name = rest
                .split_once(',')
                .map(|x| x.1)
                .unwrap_or("")
                .trim()
                .to_string();
        } else if let Some(rest) = line.strip_prefix("#EXTGENRE:") {
            genre = rest.trim().to_string();
        } else if !line.is_empty() && !line.starts_with('#') {
            let mut station = Station::basic(
                if name.is_empty() { line } else { name.as_str() },
                line,
                if genre.is_empty() {
                    "Unknown"
                } else {
                    genre.as_str()
                },
                "",
                0,
            );
            station.station_uuid = station_uuid.take();
            stations.push(station);
            name.clear();
            genre.clear();
        }
    }
    stations
}

pub fn to_json(stations: &[Station]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(stations)
}

pub fn from_json(text: &str) -> serde_json::Result<Vec<Station>> {
    serde_json::from_str(text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistFormat {
    M3u,
    Json,
}

pub fn format_for_path(path: &str) -> PlaylistFormat {
    if path
        .rsplit('.')
        .next()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        PlaylistFormat::Json
    } else {
        PlaylistFormat::M3u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_m3u_roundtrip() {
        let stations = vec![
            Station::basic("Station A", "http://a", "Synthwave", "US", 128),
            Station::basic("Station B", "http://b", "Ambient", "UK", 96),
        ];

        let m3u = to_m3u(&stations);
        let parsed = from_m3u(&m3u);
        assert_eq!(parsed.len(), 2);

        assert_eq!(parsed[0].name, "Station A");
        assert_eq!(parsed[0].url, "http://a");
        assert_eq!(parsed[0].genre, "Synthwave");

        assert_eq!(parsed[1].name, "Station B");
        assert_eq!(parsed[1].url, "http://b");
        assert_eq!(parsed[1].genre, "Ambient");
    }

    #[test]
    fn test_m3u_from_bare_urls() {
        let text = "http://a\nhttp://b\n";
        let parsed = from_m3u(text);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "http://a");
        assert_eq!(parsed[0].url, "http://a");
        assert_eq!(parsed[0].genre, "Unknown");
    }

    #[test]
    fn m3u_export_includes_radiobrowser_uuid_when_present() {
        let mut station = Station::basic("Station A", "http://a", "Synthwave", "US", 128);
        station.station_uuid = Some("uuid-a".to_string());

        let m3u = to_m3u(&[station]);
        assert!(m3u.contains("#RADIOBROWSERUUID:uuid-a"));
    }

    #[test]
    fn m3u_import_reads_radiobrowser_uuid() {
        let text = "#EXTM3U\n#RADIOBROWSERUUID:uuid-a\n#EXTINF:-1,Station A\nhttp://a\n";
        let parsed = from_m3u(text);

        assert_eq!(parsed[0].station_uuid.as_deref(), Some("uuid-a"));
    }

    #[test]
    fn m3u_import_does_not_leak_uuid_to_next_station() {
        let text =
            "#EXTM3U\n#RADIOBROWSERUUID:uuid-a\n#EXTINF:-1,A\nhttp://a\n#EXTINF:-1,B\nhttp://b\n";
        let parsed = from_m3u(text);

        assert_eq!(parsed[0].station_uuid.as_deref(), Some("uuid-a"));
        assert_eq!(parsed[1].station_uuid, None);
    }

    #[test]
    fn test_format_for_path() {
        assert_eq!(format_for_path("foo.json"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.JSON"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.m3u"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo.txt"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo"), PlaylistFormat::M3u);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut station = Station::basic("Station A", "http://a", "Synthwave", "US", 128);
        station.station_uuid = Some("uuid-a".to_string());
        station.country_code = "US".to_string();
        station.codec = "MP3".to_string();

        let json = to_json(&[station.clone()]).unwrap();
        let parsed = from_json(&json).unwrap();
        assert_eq!(parsed, vec![station]);
    }

    #[test]
    fn json_import_accepts_old_station_shape() {
        let json = r#"[
            {
                "name": "Old Station",
                "url": "http://old",
                "genre": "Ambient",
                "country": "US",
                "bitrate": 128
            }
        ]"#;

        let parsed = from_json(json).unwrap();
        assert_eq!(parsed[0].station_uuid, None);
        assert_eq!(parsed[0].country_code, "");
        assert_eq!(parsed[0].codec, "");
    }
}
