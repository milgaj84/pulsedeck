use std::path::{Path, PathBuf};

use crate::radio::Station;

pub fn export_library_m3u(
    stations: &[Station],
    dir: &Path,
    unix_time: u64,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let filepath = dir.join(format!("pulsedeck-export-{unix_time}.m3u"));
    std::fs::write(&filepath, crate::playlist::to_m3u(stations))?;
    Ok(filepath)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str, url: &str) -> Station {
        Station::basic(name, url, "Synthwave", "US", 128)
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("pulsedeck-{name}-{suffix}"))
    }

    #[test]
    fn export_library_m3u_creates_directory_and_uses_supplied_timestamp() {
        let dir = unique_temp_dir("export-m3u");
        let stations = vec![station("Station A", "http://a")];

        let path = export_library_m3u(&stations, &dir, 42).unwrap();

        assert_eq!(path, dir.join("pulsedeck-export-42.m3u"));
        assert!(path.exists());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_library_m3u_writes_playlist_content() {
        let dir = unique_temp_dir("export-content");
        let stations = vec![station("Station A", "http://a")];

        let path = export_library_m3u(&stations, &dir, 7).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();

        assert_eq!(contents, crate::playlist::to_m3u(&stations));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
