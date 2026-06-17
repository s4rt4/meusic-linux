//! Radio stations: a user-editable list seeded from the bundled JSON on first
//! run, persisted to stations.json in the config dir.

use crate::util::config_file;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Seed {
    stations: Vec<SeedStation>,
}
#[derive(Deserialize)]
struct SeedStation {
    name: String,
    url: String,
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("st-{n:x}")
}

fn seed_stations() -> Vec<Station> {
    let raw = include_str!("../assets/radio-stations.json");
    let seed: Seed = serde_json::from_str(raw).unwrap_or(Seed { stations: vec![] });
    seed.stations
        .into_iter()
        .enumerate()
        .map(|(i, s)| Station {
            id: format!("seed-{i}"),
            name: s.name,
            url: s.url,
        })
        .collect()
}

/// Load the station list (seeding + persisting the bundled list on first run).
pub fn load() -> Vec<Station> {
    if let Some(p) = config_file("stations.json") {
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(list) = serde_json::from_str::<Vec<Station>>(&s) {
                return list;
            }
        }
    }
    let seed = seed_stations();
    save(&seed);
    seed
}

pub fn save(list: &[Station]) {
    if let Some(p) = config_file("stations.json") {
        if let Ok(s) = serde_json::to_string_pretty(list) {
            let _ = std::fs::write(p, s);
        }
    }
}

pub fn new_station(name: &str, url: &str) -> Station {
    Station {
        id: new_id(),
        name: name.trim().to_string(),
        url: url.trim().to_string(),
    }
}

/// Up to two uppercase initials for a station tile (e.g. "Gen FM" → "GF").
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut s = String::new();
    if let Some(w) = words.first().and_then(|w| w.chars().next()) {
        s.push(w);
    }
    if let Some(w) = words.get(1).and_then(|w| w.chars().next()) {
        s.push(w);
    }
    if s.is_empty() {
        s.push('?');
    }
    s.to_uppercase()
}
