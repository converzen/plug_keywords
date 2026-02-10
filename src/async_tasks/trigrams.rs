use anyhow::anyhow;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

pub trait Named {
    fn names(&self) -> &[String];
}

#[derive(Debug)]
pub struct Trigrams<T: Named + Clone + Serialize> {
    item_map: HashMap<String, T>,
    trigrams: Vec<(String, HashSet<String>)>,
}

impl<T: Named + Clone + Serialize + Debug> Trigrams<T> {
    pub fn new(items: Vec<T>) -> anyhow::Result<Self> {
        let mut trigram_list = Vec::new();
        let mut item_map = HashMap::new();
        for item in items {
            let names = item.names();
            let tag = names
                .first()
                .ok_or_else(|| anyhow!("no names found"))?
                .to_string();
            names
                .iter()
                .for_each(|name| trigram_list.push((tag.clone(), trigrams(name))));
            item_map.insert(tag, item);
        }

        Ok(Self {
            item_map,
            trigrams: trigram_list,
        })
    }

    pub fn search(&self, str: &str, n_first: usize, min_score: f64) -> Vec<Match<T>> {
        let cmp = trigrams(str);
        let mut non_zero_matches = if min_score > 0.0 {
            self.trigrams
                .iter()
                .map(|(name, trigrams)| (name, trigram_similarity(trigrams, &cmp)))
                .filter(|(_name, score)| *score >= min_score)
                .collect::<Vec<_>>()
        } else {
            // returning zero score results makes no sense
            self.trigrams
                .iter()
                .map(|(name, trigrams)| (name, trigram_similarity(trigrams, &cmp)))
                .filter(|(_name, score)| *score > min_score)
                .collect::<Vec<_>>()
        };

        non_zero_matches.sort_by(|a, b| {
            // reverse sort order, sort descending
            b.1.partial_cmp(&a.1).unwrap()
        });

        non_zero_matches
            .into_iter()
            .take(n_first)
            .map(|(name, score)| Match {
                item: self
                    .item_map
                    .get(name.as_str())
                    .expect("name should exist in hashmap")
                    .clone(),
                score,
            })
            .collect()
    }
}

#[derive(Clone, Serialize)]
pub struct Match<T> {
    pub item: T,
    pub score: f64,
}

fn trigrams(s: &str) -> HashSet<String> {
    let s = s.to_lowercase();
    let s = format!("  {s}  "); // pad with spaces like pg_trgm
    s.chars()
        .collect::<Vec<_>>()
        .windows(3)
        .map(|w| w.iter().collect())
        .collect()
}

fn trigram_similarity(trigram: &HashSet<String>, cmp: &HashSet<String>) -> f64 {
    let intersection = trigram.intersection(cmp).count() as f64;
    let union = trigram.union(cmp).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}
