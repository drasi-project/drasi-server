use anyhow::{ensure, Context, Result};
use gpu_contracts::{decimal, Candidate};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitReceipt {
    pub decision_id: Uuid,
    #[serde(with = "decimal")]
    pub plan_version: u64,
    #[serde(with = "decimal")]
    pub current_plan_version: u64,
    pub status: String,
}

#[derive(Debug)]
pub enum Outcome {
    Committed(CommitReceipt),
    Conflict,
    Superseded,
}

pub struct PlanWriter {
    client: reqwest::Client,
    endpoint: String,
    token: String,
}

impl PlanWriter {
    pub fn new(endpoint: String, token: String) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(5))
                .build()?,
            endpoint,
            token,
        })
    }

    // The lifecycle-owned reaction must await this mutable call before admitting another write.
    // A changed desired ID cancels retries, not an HTTP request that might already have committed.
    pub async fn write(
        &mut self,
        candidate: &Candidate,
        desired: &mut watch::Receiver<Option<Uuid>>,
    ) -> Result<Outcome> {
        for attempt in 0..5 {
            if *desired.borrow_and_update() != Some(candidate.decision_id) {
                return Ok(Outcome::Superseded);
            }
            let response = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.token)
                .json(candidate)
                .send()
                .await;
            let failure = match response {
                Ok(response) if response.status().is_success() => {
                    let receipt: CommitReceipt =
                        response.json().await.context("invalid plan receipt")?;
                    ensure!(
                        receipt.decision_id == candidate.decision_id
                            && ["committed", "already_committed"]
                                .contains(&receipt.status.as_str())
                            && receipt.plan_version > candidate.expected_plan_version
                            && receipt.current_plan_version >= receipt.plan_version,
                        "inconsistent plan receipt"
                    );
                    return Ok(Outcome::Committed(receipt));
                }
                Ok(response) if response.status() == reqwest::StatusCode::CONFLICT => {
                    return Ok(Outcome::Conflict)
                }
                Ok(response) if response.status().is_server_error() => {
                    format!("plan endpoint returned {}", response.status())
                }
                Ok(response) => anyhow::bail!("plan rejected with HTTP {}", response.status()),
                Err(error) => format!("plan transport failed: {error}"),
            };
            tracing::warn!(decision_id=%candidate.decision_id,attempt=attempt+1,error=%failure,"plan write failed");
            ensure!(attempt < 4, "plan write exhausted five attempts: {failure}");
            let delay = Duration::from_millis((200u64 << attempt).min(2000));
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                changed = desired.changed() => {
                    changed.context("plan reaction stopped")?;
                    if *desired.borrow_and_update() != Some(candidate.decision_id) { return Ok(Outcome::Superseded); }
                }
            }
        }
        unreachable!("fifth failure returns above")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn candidate() -> Candidate {
        Candidate {
            decision_id: Uuid::new_v4(),
            expected_plan_version: 1,
            config_fingerprint: "config".into(),
            policy_signature: "policy".into(),
            policy_bundle_hash: "bundle".into(),
            scheduling_signature: "schedule".into(),
            assignments: vec![],
            decision_details: serde_json::json!({}),
        }
    }
    async fn serve(router: Router) -> Result<(String, tokio::task::JoinHandle<()>)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}/plans", listener.local_addr()?);
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("test HTTP server failed");
        });
        Ok((endpoint, task))
    }
    #[tokio::test]
    async fn conflict_is_terminal_and_never_retried() -> Result<()> {
        let calls = Arc::new(AtomicUsize::new(0));
        let router = Router::new()
            .route(
                "/plans",
                post(|State(c): State<Arc<AtomicUsize>>| async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    StatusCode::CONFLICT
                }),
            )
            .with_state(calls.clone());
        let (endpoint, task) = serve(router).await?;
        let c = candidate();
        let (_sender, mut desired) = watch::channel(Some(c.decision_id));
        let result = PlanWriter::new(endpoint, "test".into())?
            .write(&c, &mut desired)
            .await;
        task.abort();
        let _ = task.await;
        assert!(matches!(result?, Outcome::Conflict));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        Ok(())
    }
    #[tokio::test]
    async fn retry_keeps_identity_and_reads_actual_commit_receipt() -> Result<()> {
        let calls = Arc::new(AtomicUsize::new(0));
        let router=Router::new().route("/plans",post(|State(n):State<Arc<AtomicUsize>>,Json(c):Json<Candidate>|async move {
            if n.fetch_add(1,Ordering::SeqCst)==0 { return StatusCode::SERVICE_UNAVAILABLE.into_response(); }
            Json(serde_json::json!({"decision_id":c.decision_id,"plan_version":"2","current_plan_version":"2","status":"committed"})).into_response()
        })).with_state(calls.clone());
        let (endpoint, task) = serve(router).await?;
        let c = candidate();
        let (_sender, mut desired) = watch::channel(Some(c.decision_id));
        let result = PlanWriter::new(endpoint, "test".into())?
            .write(&c, &mut desired)
            .await;
        task.abort();
        let _ = task.await;
        let Outcome::Committed(receipt) = result? else {
            anyhow::bail!("missing receipt");
        };
        assert_eq!(receipt.decision_id, c.decision_id);
        assert_eq!(receipt.plan_version, 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        Ok(())
    }
    #[tokio::test]
    async fn retracted_candidate_sends_nothing() -> Result<()> {
        let c = candidate();
        let (_sender, mut desired) = watch::channel(None);
        let result = PlanWriter::new("http://127.0.0.1:1/plans".into(), "test".into())?
            .write(&c, &mut desired)
            .await?;
        assert!(matches!(result, Outcome::Superseded));
        Ok(())
    }
}
