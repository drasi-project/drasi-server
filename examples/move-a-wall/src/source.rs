use crate::{
    model::{self, Entity, Scene},
    wire::{self, Emitter, Records},
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use drasi_lib::computation::v1::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{oneshot, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputRecord {
    Entity {
        revision: u64,
        entity: Entity,
    },
    Clock {
        revision: u64,
        members: BTreeMap<String, u64>,
        changed: Vec<String>,
        command: String,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Put { entity: Entity },
    Delete { id: String },
    Reset,
    Clear,
}

pub struct Store {
    pub revision: u64,
    scene: Scene,
    versions: BTreeMap<String, u64>,
    emitter: Emitter,
    sender: mpsc::Sender<OutputEnvelope>,
}
pub struct SceneSource {
    descriptor: ComponentDescriptor,
    receiver: mpsc::Receiver<OutputEnvelope>,
    started: bool,
    http_store: Option<Arc<Mutex<Store>>>,
    server: Option<(
        oneshot::Sender<()>,
        tokio::task::JoinHandle<std::io::Result<()>>,
    )>,
}

pub fn channel() -> Result<(Store, SceneSource)> {
    let (sender, receiver) = mpsc::channel(32);
    Ok((
        Store {
            revision: 0,
            scene: Scene::new(),
            versions: BTreeMap::new(),
            emitter: Emitter::new("scene/out")?,
            sender,
        },
        SceneSource {
            descriptor: ComponentDescriptor::try_new(
                ComponentId::try_new("scene")?,
                vec![wire::port("out", PortDirection::Output, false)?],
            )?,
            receiver,
            started: false,
            http_store: None,
            server: None,
        },
    ))
}
impl SceneSource {
    pub fn hosted(descriptor: ComponentDescriptor) -> Result<Self> {
        let (store, mut source) = channel()?;
        source.descriptor = descriptor;
        source.http_store = Some(Arc::new(Mutex::new(store)));
        Ok(source)
    }
}
impl Store {
    pub async fn command(&mut self, expected: u64, command: Command) -> Result<u64> {
        ensure!(
            expected == self.revision,
            "revision conflict: expected {expected}, current {}; reconnect before retrying",
            self.revision
        );
        let mut next = self.scene.clone();
        let label = match command {
            Command::Put { entity } => {
                entity.validate()?;
                if let Some(old) = next.get(&entity.id) {
                    ensure!(
                        old.label() == entity.label(),
                        "an existing entity's kind cannot change"
                    );
                }
                next.insert(entity.id.clone(), entity);
                "put"
            }
            Command::Delete { id } => {
                ensure!(next.remove(&id).is_some(), "entity {id} does not exist");
                "delete"
            }
            Command::Reset => {
                next = model::fixture();
                "reset"
            }
            Command::Clear => {
                next.clear();
                "clear"
            }
        };
        model::validate_scene(&next)?;
        if next == self.scene && self.revision > 0 {
            return Ok(self.revision);
        }
        let revision = self
            .revision
            .checked_add(1)
            .context("input revision exhausted")?;
        let changed = self
            .scene
            .keys()
            .chain(next.keys())
            .filter(|id| self.scene.get(*id) != next.get(*id))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut versions = self.versions.clone();
        versions.retain(|id, _| next.contains_key(id));
        for id in &changed {
            if next.contains_key(id) {
                versions.insert(id.clone(), revision);
            }
        }
        let mut records = Records::new();
        for entity in next.values() {
            let record = InputRecord::Entity {
                revision: versions[&entity.id],
                entity: entity.clone(),
            };
            records.insert(("SceneObject".into(), entity.id.clone()), json!({
                "id":entity.id, "active":entity.active, "payload":serde_json::to_string(&record)?
            }));
        }
        let clock = InputRecord::Clock {
            revision,
            members: versions
                .iter()
                .filter(|(id, _)| next[*id].active)
                .map(|(id, v)| (id.clone(), *v))
                .collect(),
            changed,
            command: label.into(),
        };
        records.insert(
            ("SceneObject".into(), "__clock".into()),
            json!({
                "id":"__clock","active":true,"payload":serde_json::to_string(&clock)?
            }),
        );
        // Reserve capacity before mutating the source's published state.
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.sender.clone().reserve_owned(),
        )
        .await
        .context("source backpressure: command was not accepted")?
        .context("scene source is stopped")?;
        let mut output = self.emitter.replace(records, None)?;
        ensure!(
            output.len() == 1,
            "a command must produce one input envelope"
        );
        self.scene = next;
        self.versions = versions;
        self.revision = revision;
        permit.send(output.remove(0));
        Ok(revision)
    }
}

#[async_trait]
impl ComputationComponent for SceneSource {
    fn descriptor(&self) -> &ComponentDescriptor {
        &self.descriptor
    }
    fn configuration(&self) -> Result<serde_json::Value> {
        Ok(json!({}))
    }
    async fn start(&mut self) -> Result<()> {
        ensure!(
            !self.started,
            "source restart requires a new graph and explicit bootstrap"
        );
        if let Some(store) = &self.http_store {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:8423").await?;
            store.lock().await.command(0, Command::Reset).await?;
            let app = Router::new()
                .route("/commands", post(command))
                .route("/health", get(|| async { Json(json!({"source":"scene"})) }))
                .layer(DefaultBodyLimit::max(64 * 1024))
                .with_state(store.clone());
            let (stop, stopped) = oneshot::channel();
            let task = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stopped.await;
                    })
                    .await
            });
            self.server = Some((stop, task));
        }
        self.started = true;
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        self.receiver.close();
        if let Some((stop, task)) = self.server.take() {
            let _ = stop.send(());
            task.await??;
        }
        Ok(())
    }
}
#[async_trait]
impl EnvelopeSource for SceneSource {
    async fn next(&mut self) -> Result<Option<OutputEnvelope>> {
        if let Some((_, task)) = self.server.as_ref() {
            ensure!(
                !task.is_finished(),
                "scene command listener stopped unexpectedly"
            );
        }
        Ok(self.receiver.recv().await)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    expected_revision: u64,
    command: Command,
}
async fn command(
    State(store): State<Arc<Mutex<Store>>>,
    headers: HeaderMap,
    payload: std::result::Result<Json<Request>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if headers.get("x-wall-command").and_then(|h| h.to_str().ok()) != Some("1")
        || headers
            .get("origin")
            .is_some_and(|h| h != "http://127.0.0.1:5421")
    {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"commands require X-Wall-Command: 1 and origin http://127.0.0.1:5421"}))).into_response();
    }
    let request = match payload {
        Ok(Json(request)) => request,
        Err(error) => {
            tracing::warn!("Malformed command: {error}");
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error":error.to_string()})),
            )
                .into_response();
        }
    };
    match store
        .lock()
        .await
        .command(request.expected_revision, request.command)
        .await
    {
        Ok(revision) => Json(json!({"accepted_revision":revision})).into_response(),
        Err(error) => {
            tracing::warn!("Command rejected: {error:#}");
            (
                StatusCode::CONFLICT,
                Json(json!({"error":format!("{error:#}")})),
            )
                .into_response()
        }
    }
}
