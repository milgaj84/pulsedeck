use crate::radio::Station;

pub fn to_m3u(stations: &[Station]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for s in stations {
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
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            name = rest
                .split_once(',')
                .map(|x| x.1)
                .unwrap_or("")
                .trim()
                .to_string();
        } else if let Some(rest) = line.strip_prefix("#EXTGENRE:") {
            genre = rest.trim().to_string();
        } else if !line.is_empty() && !line.starts_with('#') {
            stations.push(Station::basic(
                if name.is_empty() {
                    line.to_string()
                } else {
                    name.clone()
                },
                line.to_string(),
                if genre.is_empty() {
                    "Unknown".to_string()
                } else {
                    genre.clone()
                },
                String::new(),
                0,
            ));
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
    fn test_format_for_path() {
        assert_eq!(format_for_path("foo.json"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.JSON"), PlaylistFormat::Json);
        assert_eq!(format_for_path("foo.m3u"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo.txt"), PlaylistFormat::M3u);
        assert_eq!(format_for_path("foo"), PlaylistFormat::M3u);
    }

    #[test]
    fn test_json_roundtrip() {
        let stations = vec![Station::basic(
            "Station A",
            "http://a",
            "Synthwave",
            "US",
            128,
        )];

        let json = to_json(&stations).unwrap();
        let parsed = from_json(&json).unwrap();
        assert_eq!(parsed, stations);
    }
}
