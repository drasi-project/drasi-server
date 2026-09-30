use anyhow::{ensure, Context, Result};
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::process::{Child, Command};

/// Owns only the stock Server child; it never constructs a DrasiLib instance.
pub struct StockProcess {
    program: PathBuf,
    arguments: Vec<OsString>,
    child: Option<Child>,
}

impl StockProcess {
    pub fn new(program: PathBuf, arguments: Vec<OsString>) -> Self {
        Self {
            program,
            arguments,
            child: None,
        }
    }

    pub fn start(&mut self) -> Result<u32> {
        ensure!(
            self.child.is_none(),
            "stock Server child is still owned; stop it before reconstruction"
        );
        let child = Command::new(&self.program)
            .args(&self.arguments)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("could not launch stock drasi-server")?;
        let pid = child.id().context("stock Server child has no process id")?;
        self.child = Some(child);
        Ok(pid)
    }

    pub fn running(&mut self) -> Result<bool> {
        let Some(child) = self.child.as_mut() else {
            return Ok(false);
        };
        if let Some(status) = child.try_wait().context("could not inspect stock Server")? {
            anyhow::bail!("stock Server exited unexpectedly: {status}");
        }
        Ok(true)
    }

    pub async fn stop(&mut self, deadline: Duration) -> Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };
        if let Some(status) = child.try_wait().context("could not inspect stock Server")? {
            self.child = None;
            return clean_exit(status);
        }
        let pid = child.id().context("stock Server child has no process id")?;
        let signaled = Command::new("/bin/kill")
            .arg("-INT")
            .arg(pid.to_string())
            .status()
            .await
            .context("could not signal stock Server")?;
        if !signaled.success() {
            // The child can finish between try_wait and delivery of SIGINT.
            if let Some(status) = child.try_wait()? {
                self.child = None;
                return clean_exit(status);
            }
            anyhow::bail!("SIGINT delivery to owned stock Server child {pid} failed: {signaled}");
        }
        match tokio::time::timeout(deadline, child.wait()).await {
            Ok(status) => {
                let status = status.context("could not reap stock Server")?;
                self.child = None;
                clean_exit(status)
            }
            Err(_) => {
                child
                    .kill()
                    .await
                    .context("could not reap timed-out stock Server")?;
                self.child = None;
                anyhow::bail!("stock Server exceeded its graceful shutdown deadline and was killed")
            }
        }
    }
}

fn clean_exit(status: ExitStatus) -> Result<()> {
    ensure!(
        status.code() == Some(0),
        "stock Server did not exit cleanly: {status}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn child_fixture() -> Result<()> {
        let Ok(ready) = std::env::var("GPU_STOCK_CHILD_READY") else {
            return Ok(());
        };
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        tokio::fs::write(ready, "ready").await?;
        signal.recv().await.context("SIGINT stream ended")?;
        Ok(())
    }

    #[tokio::test]
    async fn owns_one_child_and_requires_clean_shutdown() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let ready = temporary.path().join("ready");
        // The shell only sets the fixture environment, then exec preserves its PID.
        let mut process = StockProcess::new(
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "export GPU_STOCK_CHILD_READY=\"$1\"; exec \"$2\" --exact process::tests::child_fixture --nocapture".into(),
                "fixture".into(),
                ready.as_os_str().to_owned(),
                std::env::current_exe()?.into_os_string(),
            ],
        );
        assert!(!process.running()?);
        process.start()?;
        assert!(process.start().is_err());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                ensure!(process.running()?, "fixture did not start");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        assert!(process.running()?);
        process.stop(Duration::from_secs(5)).await?;
        assert!(!process.running()?);
        process.stop(Duration::from_secs(5)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn unexpected_nonzero_exit_is_not_success() -> Result<()> {
        let mut process = StockProcess::new("/bin/sh".into(), vec!["-c".into(), "exit 7".into()]);
        process.start()?;
        process.child.as_mut().unwrap().wait().await?;
        assert!(process.running().is_err());
        assert!(process.stop(Duration::from_secs(1)).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn forced_shutdown_remains_an_error() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let ready = temporary.path().join("ready");
        let mut process = StockProcess::new(
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "trap '' INT; printf ready > \"$1\"; exec sleep 30".into(),
                "fixture".into(),
                ready.as_os_str().to_owned(),
            ],
        );
        process.start()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                ensure!(process.running()?, "fixture did not start");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        let failure = process.stop(Duration::from_millis(50)).await.unwrap_err();
        assert!(failure.to_string().contains("was killed"));
        assert!(!process.running()?);
        Ok(())
    }
}
