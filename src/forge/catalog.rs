// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Component catalog: maps a config `kind` to the crate + descriptor needed to
//! statically link and instantiate that component in a forged binary.
//!
//! Seeded from `catalog.json` (embedded at build time). This is intentionally
//! data-only; see `REUSE_GAPS.md` for the plan to generate and publish it from
//! component crate metadata per release.

use anyhow::{Context, Result};
use serde::Deserialize;

const CATALOG_JSON: &str = include_str!("catalog.json");

/// A source or reaction component entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub kind: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub version: String,
    #[serde(default)]
    pub local_only: bool,
    pub descriptor: String,
    pub dir: String,
}

impl Entry {
    /// The Rust module path for the crate (dashes become underscores).
    pub fn module(&self) -> String {
        self.crate_name.replace('-', "_")
    }
}

/// An index provider entry (constructed via a provider struct, not a descriptor).
#[derive(Debug, Clone, Deserialize)]
pub struct IndexEntry {
    pub kind: String,
    #[serde(rename = "crate")]
    pub crate_name: String,
    pub version: String,
    #[serde(default)]
    pub local_only: bool,
    pub provider: String,
    pub dir: String,
}

impl IndexEntry {
    pub fn module(&self) -> String {
        self.crate_name.replace('-', "_")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub sources: Vec<Entry>,
    #[serde(default)]
    pub reactions: Vec<Entry>,
    #[serde(default)]
    pub indexes: Vec<IndexEntry>,
    #[serde(default)]
    pub bootstrappers: Vec<Entry>,
    #[serde(default, rename = "secretStores")]
    pub secret_stores: Vec<Entry>,
    #[serde(default, rename = "identityProviders")]
    pub identity_providers: Vec<Entry>,
    #[serde(default, rename = "stateStores")]
    pub state_stores: Vec<IndexEntry>,
    pub wal: IndexEntry,
}

impl Catalog {
    /// Load the embedded catalog.
    pub fn load() -> Result<Self> {
        serde_json::from_str(CATALOG_JSON).context("failed to parse embedded forge catalog.json")
    }

    pub fn source(&self, kind: &str) -> Option<&Entry> {
        self.sources.iter().find(|e| e.kind == kind)
    }

    pub fn reaction(&self, kind: &str) -> Option<&Entry> {
        self.reactions.iter().find(|e| e.kind == kind)
    }

    pub fn index(&self, kind: &str) -> Option<&IndexEntry> {
        self.indexes.iter().find(|e| e.kind == kind)
    }

    pub fn bootstrapper(&self, kind: &str) -> Option<&Entry> {
        self.bootstrappers.iter().find(|e| e.kind == kind)
    }

    pub fn secret_store(&self, kind: &str) -> Option<&Entry> {
        self.secret_stores.iter().find(|e| e.kind == kind)
    }

    pub fn identity_provider(&self, kind: &str) -> Option<&Entry> {
        self.identity_providers.iter().find(|e| e.kind == kind)
    }

    pub fn state_store(&self, kind: &str) -> Option<&IndexEntry> {
        self.state_stores.iter().find(|e| e.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_has_known_kinds() {
        let cat = Catalog::load().expect("catalog should parse");
        assert!(cat.source("mock").is_some(), "mock source present");
        assert!(cat.reaction("log").is_some(), "log reaction present");
        assert!(cat.index("rocksdb").is_some(), "rocksdb index present");
        // module derivation
        assert_eq!(cat.source("mock").unwrap().module(), "drasi_source_mock");
        assert!(cat.source("otel").is_some());
        assert!(cat.source("websocket").is_some());
        assert!(cat.reaction("eventgrid").is_some());
        assert!(cat.source("here-traffic").unwrap().local_only);
        assert!(cat.reaction("aws-sqs").unwrap().local_only);
        assert!(cat.reaction("snapshot-test").unwrap().local_only);
        assert_eq!(cat.wal.crate_name, "drasi-wal-redb");
    }

    #[test]
    fn catalog_covers_all_plugin_types() {
        let cat = Catalog::load().expect("catalog should parse");
        assert!(
            cat.bootstrapper("postgres").is_some(),
            "postgres bootstrapper present"
        );
        assert!(
            cat.secret_store("file").is_some(),
            "file secret store present"
        );
        assert!(
            cat.identity_provider("azure").is_some(),
            "azure identity provider present"
        );
        assert!(
            cat.state_store("redb").is_some(),
            "redb state store present"
        );
        assert_eq!(
            cat.bootstrapper("postgres").unwrap().module(),
            "drasi_bootstrap_postgres"
        );
        assert_eq!(
            cat.state_store("redb").unwrap().provider,
            "RedbStateStoreProvider"
        );
    }
}
