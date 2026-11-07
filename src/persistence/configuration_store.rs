// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::Write,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use drasi_lib::management::{
    AcceptanceReceipt, AcceptedComponentCreation, AcceptedComponentUpdate, CommittedConfiguration,
    ComponentUpdateCompletion, ConfigurationSession, ConfigurationSnapshotSummary,
    ConfigurationStore, ConfigurationStoreGuarantees, DesiredInstance, ManagementError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FORMAT: u32 = 3;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: u32,
    instances: BTreeMap<String, Records>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            format: FORMAT,
            instances: BTreeMap::new(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Records {
    current: CommittedConfiguration,
    receipts: BTreeMap<String, Receipt>,
    snapshots: BTreeMap<String, CommittedConfiguration>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    digest: [u8; 32],
    receipt: AcceptanceReceipt,
}

struct Owner {
    path: PathBuf,
    lock: File,
    writes: Mutex<()>,
    sessions: Mutex<BTreeSet<String>>,
    durability_unconfirmed: AtomicBool,
}

impl Drop for Owner {
    fn drop(&mut self) {
        // A forked child can retain this open-file description until exec.
        // Release ownership explicitly instead of waiting for its last descriptor.
        if let Err(error) = self.lock.unlock() {
            log::error!(
                "Could not unlock configuration store {}: {error}",
                self.path.display()
            );
        }
    }
}

/// File-backed implementation of Core's configuration owner. This file is not
/// an authoring DTO or an identity permit supplied to a component-creation API.
#[derive(Clone)]
pub struct YamlConfigurationStore(Arc<Owner>);

impl YamlConfigurationStore {
    pub fn new(path: PathBuf, initialize_if_missing: bool) -> Result<Self> {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        let parent = path
            .parent()
            .context("configuration store needs a parent directory")?;
        std::fs::create_dir_all(parent).context("create configuration store directory")?;
        let mut lock_name = path.as_os_str().to_os_string();
        lock_name.push(".lock");
        let lock = private_file(&PathBuf::from(lock_name), false)?;
        lock.try_lock()
            .context("configuration store is already owned or cannot be locked")?;
        let owner = Arc::new(Owner {
            path,
            lock,
            writes: Mutex::new(()),
            sessions: Mutex::new(BTreeSet::new()),
            durability_unconfirmed: AtomicBool::new(true),
        });
        match std::fs::metadata(&owner.path) {
            Ok(_) => {
                owner.load()?;
            }

            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                anyhow::ensure!(
                    initialize_if_missing,
                    "Configuration store is missing; refusing to replace it with empty state"
                );
                owner.save(&Document::default())?;
            }
            Err(error) => return Err(error).context("inspect configuration store"),
        }
        Ok(Self(owner))
    }

    pub async fn configured_instance_ids(&self) -> Result<Vec<String>> {
        let owner = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = owner
                .writes
                .lock()
                .map_err(|_| anyhow::anyhow!("configuration writer failed"))?;
            Ok(owner.load()?.instances.into_keys().collect())
        })
        .await
        .context("configuration discovery worker failed")?
    }

    pub async fn initialize_if_absent_with(
        &self,
        instance: &str,
        request_id: &str,
        seed: impl FnOnce() -> Result<DesiredInstance> + Send,
    ) -> Result<bool> {
        let session = self.open(instance).await?;
        let result = async {
            if self
                .configured_instance_ids()
                .await?
                .iter()
                .any(|id| id == instance)
            {
                return Ok(false);
            }
            session
                .commit(0, request_id, &seed()?.normalized()?, None, &[])
                .await?;
            Ok(true)
        }
        .await;
        let closed = session.close().await;
        match (result, closed) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(close)) => {
                Err(error.context(format!("configuration close also failed: {close:#}")))
            }
        }
    }
}

fn private_file(path: &Path, create_new: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .context("open private configuration file")
}

pub(crate) fn atomic_write_yaml(path: &Path, document: &impl Serialize) -> Result<()> {
    let path = std::path::absolute(path).context("resolve configuration output path")?;
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let staging = PathBuf::from(name);
    let result = (|| {
        let mut file = private_file(&staging, true)?;
        file.write_all(serde_yaml::to_string(document)?.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&staging, &path)?;
        #[cfg(unix)]
        File::open(path.parent().context("configuration file has no parent")?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(error) = std::fs::remove_file(&staging) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::error!(
                    "Could not remove configuration staging file {}: {error}",
                    staging.display()
                );
            }
        }
    }
    result
}

impl Owner {
    fn load(&self) -> Result<Document> {
        if self.durability_unconfirmed.load(Ordering::Acquire) {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.path)?
                .sync_all()
                .context("confirm configuration file durability")?;
            #[cfg(unix)]
            File::open(
                self.path
                    .parent()
                    .context("configuration file has no parent")?,
            )?
            .sync_all()
            .context("confirm configuration directory durability")?;
            self.durability_unconfirmed.store(false, Ordering::Release);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            anyhow::ensure!(
                std::fs::metadata(&self.path)?.permissions().mode() & 0o077 == 0,
                "configuration records require owner-only permissions (chmod 600)"
            );
        }
        #[derive(Deserialize)]
        struct Header {
            format: u32,
        }
        let bytes = std::fs::read(&self.path)?;
        let header: Header = serde_yaml::from_slice(&bytes)
            .context("read configuration format; incompatible or damaged data is never reset")?;
        anyhow::ensure!(
            header.format == FORMAT,
            "unsupported configuration record format {}; use a fresh store, leaving existing data untouched",
            header.format
        );
        let document: Document = serde_yaml::from_slice(&bytes)
            .context("read configuration records; incompatible or damaged data is never reset")?;
        Ok(document)
    }

    fn save(&self, document: &Document) -> Result<()> {
        self.durability_unconfirmed.store(true, Ordering::Release);
        atomic_write_yaml(&self.path, document)?;
        self.durability_unconfirmed.store(false, Ordering::Release);
        Ok(())
    }
}

struct Session {
    owner: Arc<Owner>,
    instance: String,
    closed: AtomicBool,
}

impl Session {
    async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        operation: impl FnOnce(&Self, &mut Document) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let session = self.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = session
                .owner
                .writes
                .lock()
                .map_err(|_| anyhow::anyhow!("configuration writer failed"))?;
            anyhow::ensure!(
                !session.closed.load(Ordering::Acquire),
                ManagementError::Closed
            );
            let mut document = session.owner.load()?;
            operation(&session, &mut document)
        })
        .await
        .context("configuration storage worker failed")?
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            match self.owner.sessions.lock() {
                Ok(mut sessions) => {
                    sessions.remove(&self.instance);
                }
                Err(_) => log::error!(
                    "Configuration ownership cleanup failed for '{}'",
                    self.instance
                ),
            }
        }
    }
}

struct SessionHandle(Arc<Session>);

#[async_trait]
impl ConfigurationStore for YamlConfigurationStore {
    fn guarantees(&self) -> ConfigurationStoreGuarantees {
        ConfigurationStoreGuarantees::DURABLE
    }

    async fn open(&self, instance_id: &str) -> Result<Arc<dyn ConfigurationSession>> {
        drasi_lib::computation::v1::validate_identifier("configuration instance", instance_id)?;
        let mut sessions = self
            .0
            .sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("configuration ownership registry failed"))?;
        anyhow::ensure!(
            sessions.insert(instance_id.to_owned()),
            ManagementError::AlreadyOwned(instance_id.into())
        );
        Ok(Arc::new(SessionHandle(Arc::new(Session {
            owner: self.0.clone(),
            instance: instance_id.into(),
            closed: AtomicBool::new(false),
        }))))
    }
}

#[async_trait]
impl ConfigurationSession for SessionHandle {
    async fn load(&self) -> Result<CommittedConfiguration> {
        self.0
            .run(|session, document| {
                Ok(document
                    .instances
                    .get(&session.instance)
                    .map(|records| records.current.clone())
                    .unwrap_or_default())
            })
            .await
    }

    async fn commit(
        &self,
        expected_revision: u64,
        request_id: &str,
        desired: &DesiredInstance,
        creation: Option<&AcceptedComponentCreation>,
        updates: &[AcceptedComponentUpdate],
    ) -> Result<AcceptanceReceipt> {
        anyhow::ensure!(
            !request_id.is_empty() && request_id.len() <= 1024,
            "invalid configuration request ID"
        );
        let desired = desired.clone().normalized()?;
        let request_id = request_id.to_owned();
        let creation = creation.cloned();
        let updates = updates.to_vec();
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(&desired)?).into();
        self.0
            .run(move |session, document| {
                let records = document
                    .instances
                    .entry(session.instance.clone())
                    .or_default();
                if let Some(previous) = records.receipts.get(&request_id) {
                    anyhow::ensure!(
                        previous.digest == digest
                            && previous.receipt.creation == creation
                            && previous.receipt.updates == updates,
                        ManagementError::RequestConflict
                    );
                    return Ok(previous.receipt.clone());
                }
                anyhow::ensure!(
                    records.current.revision == expected_revision,
                    ManagementError::RevisionConflict {
                        expected: expected_revision,
                        actual: records.current.revision
                    }
                );
                let revision = if records.current.desired == desired {
                    expected_revision
                } else {
                    expected_revision
                        .checked_add(1)
                        .context("configuration revision overflow")?
                };
                let mut pending = records.current.pending_updates.clone();
                for update in &updates {
                    anyhow::ensure!(
                        !pending
                            .iter()
                            .any(|previous| previous.component_id == update.component_id),
                        "component already has an incomplete accepted update"
                    );
                    pending.push(update.clone());
                }
                pending.sort_by(|left, right| left.component_id.cmp(&right.component_id));
                records.current = CommittedConfiguration {
                    revision,
                    desired,
                    pending_updates: pending,
                };
                let receipt = AcceptanceReceipt {
                    request_id: request_id.clone(),
                    revision: records.current.revision,
                    durable: true,
                    creation,
                    updates,
                };
                records.receipts.insert(
                    request_id,
                    Receipt {
                        digest,
                        receipt: receipt.clone(),
                    },
                );
                session.owner.save(document)?;
                Ok(receipt)
            })
            .await
    }

    async fn complete_updates(
        &self,
        expected_revision: u64,
        updates: &[ComponentUpdateCompletion],
    ) -> Result<CommittedConfiguration> {
        let updates = updates.to_vec();
        self.0
            .run(move |session, document| {
                let mut current = document
                    .instances
                    .get(&session.instance)
                    .map(|records| records.current.clone())
                    .unwrap_or_default();
                anyhow::ensure!(
                    current.revision == expected_revision,
                    ManagementError::RevisionConflict {
                        expected: expected_revision,
                        actual: current.revision
                    }
                );
                for update in &updates {
                    if let Some(pending) = current
                        .pending_updates
                        .iter()
                        .find(|pending| pending.component_id == update.component_id)
                    {
                        anyhow::ensure!(
                            pending.operation_id == update.operation_id,
                            ManagementError::RequestConflict
                        );
                    }
                }
                let before = current.pending_updates.len();
                current.pending_updates.retain(|pending| {
                    !updates.iter().any(|update| {
                        update.component_id == pending.component_id
                            && update.operation_id == pending.operation_id
                    })
                });
                if current.pending_updates.len() != before {
                    document
                        .instances
                        .get_mut(&session.instance)
                        .context("accepted updates lost their configuration record")?
                        .current = current.clone();
                    session.owner.save(document)?;
                }
                Ok(current)
            })
            .await
    }

    async fn receipt(&self, request_id: &str) -> Result<Option<AcceptanceReceipt>> {
        let request_id = request_id.to_owned();
        self.0
            .run(move |session, document| {
                Ok(document
                    .instances
                    .get(&session.instance)
                    .and_then(|records| records.receipts.get(&request_id))
                    .map(|entry| entry.receipt.clone()))
            })
            .await
    }

    async fn snapshot(&self, name: &str) -> Result<CommittedConfiguration> {
        drasi_lib::computation::v1::validate_identifier("snapshot name", name)?;
        let name = name.to_owned();
        self.0
            .run(move |session, document| {
                let records = document
                    .instances
                    .get_mut(&session.instance)
                    .context("instance has no committed configuration")?;
                anyhow::ensure!(
                    !records.snapshots.contains_key(&name),
                    ManagementError::SnapshotExists(name.clone())
                );
                let current = records.current.clone();
                records.snapshots.insert(name, current.clone());
                session.owner.save(document)?;
                Ok(current)
            })
            .await
    }

    async fn load_snapshot(&self, name: &str) -> Result<Option<CommittedConfiguration>> {
        let name = name.to_owned();
        self.0
            .run(move |session, document| {
                Ok(document
                    .instances
                    .get(&session.instance)
                    .and_then(|records| records.snapshots.get(&name))
                    .cloned())
            })
            .await
    }

    async fn list_snapshots(
        &self,
        after: Option<&str>,
        limit: NonZeroUsize,
    ) -> Result<Vec<ConfigurationSnapshotSummary>> {
        let after = after.map(str::to_owned);
        self.0
            .run(move |session, document| {
                Ok(document
                    .instances
                    .get(&session.instance)
                    .into_iter()
                    .flat_map(|records| &records.snapshots)
                    .filter(|(name, _)| after.as_ref().is_none_or(|after| *name > after))
                    .take(limit.get())
                    .map(|(name, value)| ConfigurationSnapshotSummary {
                        name: name.clone(),
                        revision: value.revision,
                    })
                    .collect())
            })
            .await
    }

    async fn delete_snapshot(&self, name: &str, expected_revision: u64) -> Result<bool> {
        let name = name.to_owned();
        self.0
            .run(move |session, document| {
                let Some(records) = document.instances.get_mut(&session.instance) else {
                    return Ok(false);
                };
                let Some(snapshot) = records.snapshots.get(&name) else {
                    return Ok(false);
                };
                anyhow::ensure!(
                    snapshot.revision == expected_revision,
                    ManagementError::RevisionConflict {
                        expected: expected_revision,
                        actual: snapshot.revision,
                    }
                );
                records.snapshots.remove(&name);
                session.owner.save(document)?;
                Ok(true)
            })
            .await
    }

    async fn close(&self) -> Result<()> {
        let session = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = session
                .owner
                .writes
                .lock()
                .map_err(|_| anyhow::anyhow!("configuration writer failed"))?;
            let mut sessions = session
                .owner
                .sessions
                .lock()
                .map_err(|_| anyhow::anyhow!("configuration ownership registry failed"))?;
            if !session.closed.swap(true, Ordering::AcqRel) {
                sessions.remove(&session.instance);
            }
            Ok(())
        })
        .await
        .context("configuration close worker failed")?
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn uncertain_durability_blocks_reads_until_the_file_is_confirmed() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("configuration.yaml");
        let displaced = directory.path().join("displaced.yaml");
        let store = YamlConfigurationStore::new(path.clone(), true)?;
        store
            .0
            .durability_unconfirmed
            .store(true, Ordering::Release);
        std::fs::rename(&path, &displaced)?;
        assert!(store.0.load().is_err());
        assert!(store.0.durability_unconfirmed.load(Ordering::Acquire));
        std::fs::rename(&displaced, &path)?;
        store.0.load()?;
        assert!(!store.0.durability_unconfirmed.load(Ordering::Acquire));
        Ok(())
    }

    #[test]
    fn dropping_owner_unlocks_even_when_a_descriptor_was_inherited() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("configuration.yaml");
        let store = YamlConfigurationStore::new(path.clone(), true)?;
        let inherited_descriptor = store.0.lock.try_clone()?;
        drop(store);

        let replacement = YamlConfigurationStore::new(path, false)?;
        assert!(replacement.0.load()?.instances.is_empty());
        drop(inherited_descriptor);
        drop(replacement);
        Ok(())
    }

    #[test]
    fn atomic_writer_accepts_a_filename_without_a_parent() -> Result<()> {
        let owned = tempfile::NamedTempFile::new_in(".")?.into_temp_path();
        let filename = Path::new(owned.file_name().context("missing test filename")?);
        atomic_write_yaml(filename, &Document::default())?;
        let saved: Document = serde_yaml::from_slice(&std::fs::read(&owned)?)?;
        assert_eq!(saved.format, FORMAT);
        Ok(())
    }

    #[test]
    fn failed_replacement_removes_only_its_staging_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let destination = directory.path().join("existing-directory");
        std::fs::create_dir(&destination)?;
        assert!(atomic_write_yaml(&destination, &Document::default()).is_err());
        assert!(destination.is_dir());
        assert_eq!(std::fs::read_dir(directory.path())?.count(), 1);
        Ok(())
    }
}
