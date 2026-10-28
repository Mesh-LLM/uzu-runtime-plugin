mod http_transport;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use mesh_llm_plugin::{PluginMetadata, PluginRuntime, PluginStartupPolicy, plugin_server_info};
use std::{path::PathBuf, process::Stdio, sync::Arc};

#[derive(Parser)]
#[command(version, about = "Managed Uzu inference for Mesh (Apple Silicon)")]
struct Args {
    /// Uzu model id or converted model directory; fetched by Uzu when missing.
    #[arg(long)]
    model: String,
    /// Credential-free Uzu server binary bundled beside the plugin.
    #[arg(long)]
    engine: Option<PathBuf>,
}

fn plugin(url: String) -> mesh_llm_plugin::SimplePlugin {
    mesh_llm_plugin::plugin! {
        metadata: PluginMetadata::new("uzu-runtime-plugin", env!("CARGO_PKG_VERSION"), plugin_server_info(
            "uzu-runtime-plugin", env!("CARGO_PKG_VERSION"), "Uzu", "Local Uzu Metal inference", None::<String>)),
        startup_policy: PluginStartupPolicy::Any,
        inference: [mesh_llm_plugin::inference::provider("uzu", url)],
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        std::env::var_os("MESH_LLM_PLUGIN_ENDPOINT").is_some(),
        "launch through Mesh; no engine started"
    );
    let engine = args.engine.unwrap_or(
        std::env::current_exe()?
            .parent()
            .context("binary directory")?
            .join("uzu-server"),
    );
    ensure!(
        engine.is_file(),
        "missing credential-free uzu-server: {}",
        engine.display()
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let frontend = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let frontend_port = frontend.local_addr()?.port();
    let initialized = Arc::new(tokio::sync::Notify::new());
    let ready = initialized.clone();
    let p = plugin(format!("http://127.0.0.1:{frontend_port}/v1"))
        .on_initialize(|request, _| {
            Box::pin(async move {
                if request.host_protocol_version != mesh_llm_plugin::PROTOCOL_VERSION {
                    return Err(mesh_llm_plugin::PluginError::internal(
                        "incompatible host plugin protocol",
                    ));
                }
                Ok(())
            })
        })
        .on_initialized(move |_| {
            ready.notify_one();
            Box::pin(async { Ok(()) })
        });
    let connection = PluginRuntime::run(p);
    tokio::pin!(connection);
    tokio::select! {
        result = &mut connection => return result,
        _ = initialized.notified() => {},
        _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => anyhow::bail!("Mesh handshake timed out"),
    }
    // Dedicated shell supervisor reaps its child even if Mesh kills this plugin.
    // No command interpolation: every external argument is a separate argv item.
    let supervisor = r#"parent=$1; shift
"$@" & child=$!
cleanup() { trap - TERM INT EXIT; kill "$child" 2>/dev/null; wait "$child" 2>/dev/null; }
trap 'cleanup; exit' TERM INT
trap cleanup EXIT
while kill -0 "$parent" 2>/dev/null && kill -0 "$child" 2>/dev/null; do sleep 1; done
kill "$child" 2>/dev/null
wait "$child"
"#;
    let mut child = tokio::process::Command::new("/bin/sh")
        .args([
            "-c",
            supervisor,
            "uzu-supervisor",
            &std::process::id().to_string(),
        ])
        .arg(engine)
        .args([
            "server",
            "--model",
            &args.model,
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--no-prefix-cache",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start Uzu supervisor")?;
    let transport = http_transport::serve(frontend, format!("http://127.0.0.1:{port}"));
    tokio::pin!(transport);
    let result = tokio::select! {
        result = &mut transport => result,
        result = &mut connection => result,
        status = child.wait() => Err(anyhow::anyhow!("Uzu exited: {}", status?)),
        _ = shutdown() => Ok(()),
    };
    // Closing the connection or receiving SIGTERM ends only our owned process.
    if let Some(id) = child.id() {
        let _ = tokio::process::Command::new("/bin/kill")
            .args(["-TERM", &id.to_string()])
            .status()
            .await;
        let _ = child.wait().await;
    }
    result
}

async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler");
    tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_llm_plugin::Plugin;
    #[test]
    fn requires_model() {
        assert!(Args::try_parse_from(["uzu-plugin"]).is_err());
    }
    #[test]
    fn declares_managed_http_provider() {
        let manifest = plugin("http://127.0.0.1:1234/v1".into())
            .manifest()
            .unwrap();
        assert_eq!(manifest.endpoints.len(), 1);
        assert!(manifest.endpoints[0].managed_by_plugin);
        assert_eq!(
            manifest.endpoints[0].protocol.as_deref(),
            Some("openai_compatible")
        );
    }
}
