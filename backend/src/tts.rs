use crate::error::{AppError, AppJson, Res};
use crate::{settings, AppState};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Deserialize)]
pub struct TtsIn {
    text: String,
    voice: Option<String>,
}

fn cache_name(voice: &str, text: &str) -> String {
    let mut h = DefaultHasher::new();
    voice.hash(&mut h);
    text.hash(&mut h);
    format!("{:016x}{:04x}", h.finish(), text.len() & 0xffff)
}

pub fn cache_dir() -> PathBuf {
    let d = std::env::temp_dir().join("pa-tts");
    std::fs::create_dir_all(&d).ok();
    d
}

fn audio(bytes: Vec<u8>, ctype: &'static str) -> Response {
    ([(header::CONTENT_TYPE, ctype), (header::CACHE_CONTROL, "private, max-age=3600")], bytes).into_response()
}

async fn run(mut cmd: Command, stdin: Option<&str>) -> Result<(), String> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped()).kill_on_drop(true);
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "not-found".to_string() } else { e.to_string() })?;
    if let (Some(text), Some(mut si)) = (stdin, child.stdin.take()) {
        si.write_all(text.as_bytes()).await.map_err(|e| e.to_string())?;
    }
    let out = tokio::time::timeout(Duration::from_secs(60), child.wait_with_output())
        .await
        .map_err(|_| "timed out".to_string())?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().last().unwrap_or("non-zero exit").to_string())
    }
}

async fn edge_tts(voice: &str, text: &str, out: &Path) -> Result<(), String> {
    let args = |c: &mut Command| {
        c.arg(format!("--voice={voice}")).arg(format!("--text={text}")).arg("--rate=-5%").arg("--write-media").arg(out);
    };
    let mut c = Command::new("edge-tts");
    args(&mut c);
    match run(c, None).await {
        Err(e) if e == "not-found" => {
            let mut c = Command::new("python3");
            c.args(["-m", "edge_tts"]);
            args(&mut c);
            run(c, None).await.map_err(|e| if e == "not-found" { "edge-tts is not installed".into() } else { e })
        }
        other => other,
    }
}

async fn piper(text: &str, out: &Path) -> Result<(), String> {
    let model = std::env::var("PIPER_MODEL").map_err(|_| "PIPER_MODEL is not set".to_string())?;
    let mut c = Command::new("piper");
    c.arg("--model").arg(model).arg("--output_file").arg(out);
    run(c, Some(text)).await.map_err(|e| if e == "not-found" { "piper is not installed".into() } else { e })
}

async fn read_nonempty(p: &Path) -> Option<Vec<u8>> {
    tokio::fs::read(p).await.ok().filter(|b| !b.is_empty())
}

pub async fn speak(State(st): State<AppState>, AppJson(b): AppJson<TtsIn>) -> Res<Response> {
    let text = b.text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::bad("text cannot be empty"));
    }
    if text.len() > 20_000 {
        return Err(AppError::bad("text is too long"));
    }
    let requested = b.voice.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let voice = match requested {
        Some(v) => v,
        None => {
            let conn = st.db.lock().unwrap();
            settings::load(&conn)?.voice
        }
    };
    let dir = cache_dir();
    let name = cache_name(&voice, &text);
    let (mp3, wav) = (dir.join(format!("{name}.mp3")), dir.join(format!("{name}.wav")));
    if let Some(bytes) = read_nonempty(&mp3).await {
        return Ok(audio(bytes, "audio/mpeg"));
    }
    if let Some(bytes) = read_nonempty(&wav).await {
        return Ok(audio(bytes, "audio/wav"));
    }

    let tmp = dir.join(format!("{name}.{}.tmp.mp3", std::process::id()));
    let edge = edge_tts(&voice, &text, &tmp).await;
    if edge.is_ok() {
        if let Some(bytes) = read_nonempty(&tmp).await {
            tokio::fs::rename(&tmp, &mp3).await.ok();
            return Ok(audio(bytes, "audio/mpeg"));
        }
    }
    tokio::fs::remove_file(&tmp).await.ok();
    let edge_err = edge.err().unwrap_or_else(|| "edge-tts produced no audio".into());
    tracing::warn!("edge-tts failed: {edge_err}");

    let tmpw = dir.join(format!("{name}.{}.tmp.wav", std::process::id()));
    let pip = piper(&text, &tmpw).await;
    if pip.is_ok() {
        if let Some(bytes) = read_nonempty(&tmpw).await {
            tokio::fs::rename(&tmpw, &wav).await.ok();
            return Ok(audio(bytes, "audio/wav"));
        }
    }
    tokio::fs::remove_file(&tmpw).await.ok();
    let pip_err = pip.err().unwrap_or_else(|| "piper produced no audio".into());
    Err(AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("text-to-speech unavailable (edge-tts: {edge_err}; piper: {pip_err})"),
    ))
}
