use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;

#[derive(Debug, Deserialize)]
pub struct TranscodeParams {
    pub video: String,
    pub time: Option<f64>,
    #[serde(rename = "audioTrack")]
    pub _audio_track: Option<usize>,
    pub fmp4: Option<String>,
    pub _subtitles: Option<String>,
    #[serde(rename = "subtitlesDelay")]
    pub _subtitles_delay: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlayerParams {
    pub source: Option<String>,
    pub paused: Option<String>,
    pub time: Option<f64>,
    pub volume: Option<f32>,
    pub stop: Option<String>,
    #[serde(rename = "audioTrack")]
    pub audio_track: Option<usize>,
}

#[derive(Clone)]
struct MpvState {
    source: Option<String>,
    time: f64,
    subtitles_src: Option<String>,
    subtitles_delay: f64,
    subtitles_size: Option<f64>,
    generation: u64,
}

static MPV_STATE: Mutex<MpvState> = Mutex::const_new(MpvState {
    source: None,
    time: 0.0,
    subtitles_src: None,
    subtitles_delay: 0.0,
    subtitles_size: None,
    generation: 0,
});

fn mpv_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("STREAM_SERVER_MPV_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    ["/usr/bin/mpv", "/usr/local/bin/mpv"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn mpv_device() -> Value {
    json!({
        "name": "MPV",
        "type": "external",
        "id": "mpv",
        "usePlayerUI": true
    })
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices))
        .route("/transcode", get(transcode))
        .route("/convert", get(transcode))
        .route("/{devID}", get(get_device))
        .route("/{devID}/player", get(player_control).post(player_control))
}

pub async fn list_devices(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let devices = state.devices.read().await;
    let mut result: Vec<Value> = devices
        .iter()
        .filter_map(|device| serde_json::to_value(device).ok())
        .collect();
    if mpv_path().is_some() {
        result.push(mpv_device());
    }
    Json(result)
}

pub async fn get_device(Path(dev_id): Path<String>) -> impl IntoResponse {
    if dev_id == "mpv" && mpv_path().is_some() {
        return Json(mpv_device()).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        format!("Device {} not found", dev_id),
    )
        .into_response()
}

pub async fn transcode(Query(params): Query<TranscodeParams>) -> Response {
    let video_url = params.video;
    let offset = params.time.unwrap_or(0.0);
    let is_fmp4 = params.fmp4.is_some();

    let mut args = vec![
        "-copyts".to_string(),
        "-ss".to_string(),
        offset.to_string(),
        "-i".to_string(),
        video_url,
    ];

    args.extend(vec![
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "ultrafast".to_string(),
        "-tune".to_string(),
        "zerolatency".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-ac".to_string(),
        "2".to_string(),
        "-threads".to_string(),
        "0".to_string(),
    ]);

    if is_fmp4 {
        args.extend(vec![
            "-movflags".to_string(),
            "frag_keyframe+empty_moov".to_string(),
            "-f".to_string(),
            "mp4".to_string(),
        ]);
    } else {
        args.extend(vec!["-f".to_string(), "matroska".to_string()]);
    }

    args.push("pipe:1".to_string());

    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to spawn ffmpeg: {}", e),
            )
                .into_response();
        }
    };

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stream = ReaderStream::new(stdout);

    let content_type = if is_fmp4 {
        "video/mp4"
    } else {
        "video/x-matroska"
    };

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::TRANSFER_ENCODING, "chunked")
        .header("transferMode.dlna.org", "Streaming")
        .header(
            "contentFeatures.dlna.org",
            "DLNA.ORG_OP=01;DLNA.ORG_CI=1;DLNA.ORG_FLAGS=01300000000000000000000000000000",
        )
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
}

pub async fn player_control(
    method: axum::http::Method,
    Path(dev_id): Path<String>,
    Query(query_params): Query<PlayerParams>,
    body: Option<Json<Value>>,
) -> impl IntoResponse {
    if dev_id == "mpv" {
        if mpv_path().is_none() {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "MPV is not installed" })),
            )
                .into_response();
        }

        let updates = if method == axum::http::Method::POST {
            body.map(|Json(value)| value).unwrap_or_else(|| json!({}))
        } else {
            serde_json::to_value(&query_params).unwrap_or_else(|_| json!({}))
        };
        return update_mpv(updates).await.into_response();
    }

    let response_json = json!({
        "deviceId": dev_id,
        "status": "not_implemented",
        "params": {
            "source": query_params.source,
            "paused": query_params.paused,
            "time": query_params.time,
            "volume": query_params.volume,
            "stop": query_params.stop,
            "audio_track": query_params.audio_track
        }
    });

    Json(response_json).into_response()
}

async fn update_mpv(updates: Value) -> Json<Value> {
    let mut state = MPV_STATE.lock().await;
    let object = updates.as_object();

    if let Some(value) = object.and_then(|value| value.get("time"))
        && let Some(time) = value.as_f64()
    {
        state.time = time;
    }
    if let Some(value) = object.and_then(|value| value.get("subtitlesSrc")) {
        state.subtitles_src = value.as_str().map(str::to_owned);
    }
    if let Some(value) = object.and_then(|value| value.get("subtitlesDelay"))
        && let Some(delay) = value.as_f64()
    {
        state.subtitles_delay = delay;
    }
    if let Some(value) = object.and_then(|value| value.get("subtitlesSize")) {
        state.subtitles_size = value.as_f64();
    }

    if let Some(source_value) = object.and_then(|value| value.get("source")) {
        state.source = source_value.as_str().map(str::to_owned);
        state.generation = state.generation.wrapping_add(1);
        if state.source.is_some() {
            let generation = state.generation;
            tokio::spawn(launch_mpv_after_updates(generation));
        }
    }

    Json(mpv_status(&state))
}

fn mpv_status(state: &MpvState) -> Value {
    json!({
        "volume": null,
        "time": state.time,
        "paused": false,
        "state": if state.source.is_some() { "playing" } else { "stopped" },
        "length": null,
        "source": state.source,
        "mediaSessionId": null,
        "subtitlesSrc": state.subtitles_src,
        "subtitlesDelay": state.subtitles_delay,
        "subtitlesSize": state.subtitles_size
    })
}

async fn launch_mpv_after_updates(generation: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let state = MPV_STATE.lock().await.clone();
    if state.generation != generation {
        return;
    }
    let Some(source) = state.source else {
        return;
    };
    let Some(path) = mpv_path() else {
        tracing::warn!("MPV disappeared before playback could start");
        return;
    };

    let mut command = Command::new(path);
    command
        .arg("--no-terminal")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if state.time > 0.0 {
        command.arg(format!("--start={}", (state.time / 1000.0).floor()));
    }
    if let Some(subtitles_src) = state.subtitles_src {
        command.arg(format!("--sub-file={subtitles_src}"));
    }
    command.arg("--").arg(source);

    match command.spawn() {
        Ok(_) => tracing::info!("Started external MPV player"),
        Err(error) => tracing::error!(%error, "Failed to start external MPV player"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpv_device_matches_legacy_casting_shape() {
        assert_eq!(
            mpv_device(),
            json!({
                "name": "MPV",
                "type": "external",
                "id": "mpv",
                "usePlayerUI": true
            })
        );
    }
}
