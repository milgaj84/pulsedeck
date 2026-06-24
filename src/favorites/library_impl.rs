impl ImportPreview {
    pub fn is_empty(&self) -> bool {
        self.new_stations.is_empty()
            && self.duplicates.is_empty()
            && self.enrichments.is_empty()
            && self.skipped.is_empty()
    }
}

impl Library {
    /// Load library from disk.
    /// On first launch (no file), seeds with starter stations.
    pub fn load(seed_stations: Vec<Station>) -> Self {
        Self::load_with_policy(MissingLibraryPolicy::SeedAndSave(seed_stations))
    }

    /// Load the library for read-only/CLI use without seeding starters or writing a file.
    /// Keeps the resolved path so a later import can still persist.
    pub fn load_existing() -> Self {
        Self::load_with_policy(MissingLibraryPolicy::Empty)
    }

    fn load_with_policy(policy: MissingLibraryPolicy) -> Self {
        let path = config_path();
        let mut load_warnings = Vec::new();

        let (stations, settings, should_save_seed) = match path.as_ref() {
            Some(path) if path.exists() => {
                read_library_file_or_fallback(path, &policy, &mut load_warnings)
            }
            _ => fallback_for_missing_library(&policy),
        };

        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings,
            path,
            load_warnings,
        };
        lib.rebuild_genres();

        if should_save_seed {
            if let Err(err) = lib.save() {
                lib.load_warnings
                    .push(format!("Could not save starter library: {err}"));
            }
        }

        lib
    }

    /// Create an in-memory library for tests without touching the user's config file.
    #[cfg(test)]
    pub fn in_memory(stations: Vec<Station>) -> Self {
        let mut lib = Self {
            stations,
            available_genres: Vec::new(),
            settings: Settings::default(),
            path: None,
            load_warnings: Vec::new(),
        };
        lib.rebuild_genres();
        lib
    }

    /// Add a station to the library (deduplicates by Radio Browser UUID when present, then URL).
    /// Returns true if actually added (not a duplicate).
    pub fn add(&mut self, station: Station) -> anyhow::Result<bool> {
        if self.contains_station(&station) {
            return Ok(false);
        }
        self.stations.push(station);
        self.rebuild_genres();
        Ok(true)
    }

    /// Import a list of stations, merging by Radio Browser UUID when present, then URL.
    pub fn import_stations(&mut self, stations: Vec<Station>) -> anyhow::Result<ImportSummary> {
        let preview = self.preview_import(stations);
        self.apply_import_preview(preview, ImportMode::All)
    }

    pub fn preview_import(&self, stations: Vec<Station>) -> ImportPreview {
        let mut preview = ImportPreview {
            new_stations: Vec::new(),
            duplicates: Vec::new(),
            enrichments: Vec::new(),
            skipped: Vec::new(),
        };
        let mut virtual_library = self.stations.clone();

        for station in stations {
            if let Some(reason) = import_skip_reason(&station) {
                preview.skipped.push(ImportSkip {
                    name: import_skip_name(&station),
                    reason,
                });
                continue;
            }

            if let Some(existing) = virtual_library
                .iter_mut()
                .find(|saved| station_identity_matches(saved, &station))
            {
                let mut enriched = existing.clone();
                if enriched.enrich_from(&station) {
                    *existing = enriched;
                    preview.enrichments.push(station);
                } else {
                    preview.duplicates.push(station);
                }
            } else {
                virtual_library.push(station.clone());
                preview.new_stations.push(station);
            }
        }

        preview
    }

    pub fn apply_import_preview(
        &mut self,
        preview: ImportPreview,
        mode: ImportMode,
    ) -> anyhow::Result<ImportSummary> {
        let mut added = 0;
        let mut skipped = preview.duplicates.len() + preview.skipped.len();
        let mut enriched = 0;

        for station in preview.enrichments {
            if self.enrich_matching_station(&station) {
                enriched += 1;
            } else if mode == ImportMode::All && !self.contains_station(&station) {
                self.stations.push(station);
                added += 1;
            } else {
                skipped += 1;
            }
        }

        if mode == ImportMode::All {
            for station in preview.new_stations {
                if self.contains_station(&station) {
                    skipped += 1;
                } else {
                    self.stations.push(station);
                    added += 1;
                }
            }
        } else {
            skipped += preview.new_stations.len();
        }

        if added > 0 || enriched > 0 {
            self.rebuild_genres();
        }

        Ok(ImportSummary {
            added,
            skipped,
            enriched,
        })
    }

    pub fn apply_metadata_refresh_results(
        &mut self,
        checked: usize,
        matches: Vec<Station>,
        failed: usize,
    ) -> MetadataRefreshSummary {
        let mut summary = MetadataRefreshSummary {
            checked,
            failed,
            ..MetadataRefreshSummary::default()
        };

        for station in matches {
            if self.enrich_matching_station(&station) {
                summary.enriched += 1;
            } else {
                summary.unchanged += 1;
            }
        }

        summary.unchanged += checked.saturating_sub(summary.enriched + summary.unchanged + failed);

        if summary.enriched > 0 {
            self.rebuild_genres();
        }

        summary
    }

    /// Remove a station by URL. Returns true if removed.
    pub fn remove(&mut self, url: &str) -> anyhow::Result<bool> {
        let before = self.stations.len();
        self.stations
            .retain(|station| !station_url_matches(&station.url, url));
        let removed = self.stations.len() < before;
        if removed {
            self.rebuild_genres();
        }
        Ok(removed)
    }

    /// Dynamically rebuild unique genres list, sorting them with "All" at the front.
    pub fn rebuild_genres(&mut self) {
        let genres: std::collections::HashSet<String> = self
            .stations
            .iter()
            .map(|s| resolve_parent_genre(&s.genre).to_string())
            .filter(|g| !g.is_empty())
            .collect();

        let mut sorted: Vec<String> = genres.into_iter().collect();
        sorted.sort_by_key(|a| a.to_lowercase());
        sorted.insert(0, "All".to_string());
        self.available_genres = sorted;
    }

    /// Check if a station identity is in the library, preferring Radio Browser UUID when available.
    pub fn contains_station(&self, station: &Station) -> bool {
        self.stations
            .iter()
            .any(|saved| station_identity_matches(saved, station))
    }

    /// Enrich an already-saved matching station with fresh Radio Browser metadata.
    pub fn enrich_matching_station(&mut self, station: &Station) -> bool {
        self.stations
            .iter_mut()
            .find(|saved| station_identity_matches(saved, station))
            .is_some_and(|saved| saved.enrich_from(station))
    }

    pub fn mark_station_success(&mut self, url: &str, now: String) -> bool {
        if let Some(station) = self
            .stations
            .iter_mut()
            .find(|station| station_url_matches(&station.url, url))
        {
            station.health.last_success_at = Some(now);
            station.health.last_error_summary.clear();
            return true;
        }
        false
    }

    pub fn mark_station_failure(&mut self, url: &str, now: String, error: &str) -> bool {
        if let Some(station) = self
            .stations
            .iter_mut()
            .find(|station| station_url_matches(&station.url, url))
        {
            station.health.last_failure_at = Some(now);
            station.health.failure_count =
                Some(station.health.failure_count.unwrap_or(0).saturating_add(1));
            station.health.last_error_summary = compact_error_summary(error);
            return true;
        }
        false
    }

    /// Check if a station URL is in the library.
    pub fn contains(&self, url: &str) -> bool {
        self.stations
            .iter()
            .any(|station| station_url_matches(&station.url, url))
    }

    /// Save library to disk (best-effort).
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(ref path) = self.path else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = LibraryFile {
            version: 1,
            stations: self.stations.clone(),
            settings: self.settings.clone(),
        };

        let json = serde_json::to_string_pretty(&file)?;
        fs::write(path, json)?;

        Ok(())
    }
}

