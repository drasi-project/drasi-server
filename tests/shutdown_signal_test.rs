#![cfg(unix)]

use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{interval, timeout};

async fn assert_shutdown(signal: &str, repeat_signal: bool, stall_http: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let config_path = directory.path().join("server.yaml");
    std::fs::write(
        &config_path,
        format!("apiVersion: drasi.io/v1\nid: shutdown-test\nhost: 127.0.0.1\nport: {port}\nlogLevel: info\npersistConfig: false\nenableUi: false\nsources: []\nqueries: []\nreactions: []\n"),
    )?;
    let log_path = directory.path().join("server.log");
    let log_file = std::fs::File::create(&log_path)?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_drasi-server"))
        .arg("--config")
        .arg(&config_path)
        .arg("--plugins-dir")
        .arg(directory.path().join("plugins"))
        .env("RUST_LOG", "info")
        .current_dir(directory.path())
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .kill_on_drop(true)
        .spawn()?;

    let ready = timeout(Duration::from_secs(30), async {
        let mut poll = interval(Duration::from_millis(20));
        loop {
            poll.tick().await;
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("Server exited before startup: {status}");
            }
            if std::fs::read_to_string(&log_path)?.contains("Drasi Server started successfully") {
                return Ok::<(), anyhow::Error>(());
            }
        }
    })
    .await;
    let startup_logs = std::fs::read_to_string(&log_path)?;
    ready
        .context(format!("Server startup timed out:\n{startup_logs}"))?
        .context(format!("Server startup failed:\n{startup_logs}"))?;

    let mut pending_request = if repeat_signal || stall_http {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
            .await?;
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()?
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await?
            .error_for_status()?;
        Some(stream)
    } else {
        None
    };

    let process_id = child.id().context("Server has no process ID")?;
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(process_id.to_string())
        .status()
        .await?;
    anyhow::ensure!(status.success(), "Failed to send {signal}");

    if repeat_signal {
        timeout(Duration::from_secs(3), async {
            let mut poll = interval(Duration::from_millis(10));
            loop {
                poll.tick().await;
                if std::fs::read_to_string(&log_path)?.contains("Shutting down Drasi Server") {
                    return Ok::<(), anyhow::Error>(());
                }
            }
        })
        .await
        .context("Shutdown did not start")??;
        let status = Command::new("kill")
            .args(["-INT", &process_id.to_string()])
            .status()
            .await?;
        anyhow::ensure!(status.success(), "Failed to send second signal");
        drop(pending_request.take());
    }

    let exited = timeout(Duration::from_secs(35), child.wait()).await;
    let logs = std::fs::read_to_string(&log_path)?;
    let status = exited.context(format!("Shutdown timed out for {signal}:\n{logs}"))??;
    assert_eq!(
        status.code(),
        Some(if stall_http { 1 } else { 0 }),
        "{signal}: {status}\n{logs}"
    );
    assert_eq!(
        logs.matches("Shutting down Drasi Server").count(),
        1,
        "{logs}"
    );
    assert!(logs.contains("drasi-lib stopped successfully"), "{logs}");
    assert_eq!(
        logs.contains("Drasi Server shutdown complete"),
        !stall_http,
        "{logs}"
    );
    if stall_http {
        assert!(
            logs.contains("Web API did not drain within 5 seconds"),
            "{logs}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn sigterm_shuts_down_gracefully() -> Result<()> {
    assert_shutdown("TERM", false, false).await
}

#[tokio::test]
async fn sigint_shuts_down_gracefully() -> Result<()> {
    assert_shutdown("INT", false, false).await
}

#[tokio::test]
async fn repeated_signals_only_run_cleanup_once() -> Result<()> {
    assert_shutdown("TERM", true, false).await
}

#[tokio::test]
async fn http_drain_timeout_is_not_graceful_success() -> Result<()> {
    assert_shutdown("TERM", false, true).await
}
