# WebRTC Audio Ingest & Recording Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create an Axum web server that accepts WebRTC audio streams from a browser client and records the Opus audio into timestamped `.ogg` files.

**Architecture:** Axum serves static frontend assets and a signaling endpoint (`POST /api/offer`). The browser captures microphone audio via WebRTC and exchanges SDP via HTTP POST. The backend utilizes `webrtc` (0.21) and `rtc` (0.21) to establish an `RTCPeerConnection`, receive RTP Opus packets on the audio track, and persist them to disk using `OggWriter`.

**Tech Stack:** Rust 2024 edition, Axum 0.8, Tokio 1.x, Tower-HTTP, WebRTC 0.21, RTC 0.21, Serde, Vanilla HTML5 / Web Audio API.

**Spec:** [docs/superpowers/specs/2026-10-30-webrtc-audio-recorder-design.md](file:///home/toghrol/Documents/rust/webrtc-mta-client/docs/superpowers/specs/2026-10-30-webrtc-audio-recorder-design.md)

## Global Constraints
* Codebase: Single Rust binary crate with `src/main.rs` and modular library modules in `src/`.
* Code style: Concise, minimal abstraction (ponytail: fewest lines that fully solve the problem).
* Format: Opus audio containerized in `.ogg` files (48000 Hz, 2 channels).
* Platform: Linux (x86_64), local network/direct host binding.

---

### Task 1: Scaffolding, Axum Server & Health Check Endpoint

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `tests/health_test.rs`

**Interfaces:**
- Consumes: None (root task)
- Produces: `app() -> axum::Router` exposed from `lib.rs` for testing and binary execution.

- [ ] **Step 1: Write failing health check test**

Create `tests/health_test.rs`:
```rust
use axum::http::StatusCode;
use axum_test::TestServer;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_health_check() {
    let router = app().await;
    let server = TestServer::new(router).expect("Failed to create test server");

    let response = server.get("/api/health").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.json::<serde_json::Value>()["status"], "ok");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test health_test`
Expected: FAIL with compilation error (no `Cargo.toml` or `webrtc_mta_client` crate found).

- [ ] **Step 3: Write minimal Cargo.toml, lib.rs, and main.rs**

Create `Cargo.toml`:
```toml
[package]
name = "webrtc-mta-client"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
tokio = { version = "1.40", features = ["full"] }
tower-http = { version = "0.6", features = ["fs", "trace", "cors"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
chrono = "0.4"
webrtc = { version = "0.21.1", features = ["runtime-tokio", "crypto-ring"] }
rtc = { version = "0.21.1", features = ["crypto-ring"] }

[dev-dependencies]
axum-test = "17.0"
```

Create `src/lib.rs`:
```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
}
```

Create `src/main.rs`:
```rust
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use webrtc_mta_client::app;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let router = app().await;
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    tracing::info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test health_test`
Expected: PASS (`test test_health_check ... ok`).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/ tests/
git commit -m "feat: setup project scaffolding with axum and health check endpoint"
```

---

### Task 2: Signaling Data Types & `/api/offer` Route Validation

**Files:**
- Create: `src/signaling.rs`
- Modify: `src/lib.rs`
- Create: `tests/signaling_test.rs`

**Interfaces:**
- Consumes: `app() -> axum::Router` from Task 1.
- Produces: `SessionDescription { sdp: String, r#type: String }`, `offer_handler`.

- [ ] **Step 1: Write failing signaling validation tests**

Create `tests/signaling_test.rs`:
```rust
use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::json;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_offer_rejects_empty_payload() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.post("/api/offer").json(&json!({})).await;
    assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_offer_rejects_malformed_sdp() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.post("/api/offer").json(&json!({
        "sdp": "invalid-sdp-content",
        "type": "offer"
    })).await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test signaling_test`
Expected: FAIL with 404 or compilation error (endpoint `/api/offer` doesn't exist).

- [ ] **Step 3: Implement signaling validation and data models**

Create `src/signaling.rs`:
```rust
use axum::{http::StatusCode, Json};
use rtc::peer_connection::sdp::RTCSessionDescription;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp: String,
    pub r#type: String,
}

pub async fn offer_handler(
    Json(payload): Json<SessionDescription>,
) -> Result<Json<SessionDescription>, StatusCode> {
    if payload.r#type != "offer" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify SDP parses as a valid session description
    RTCSessionDescription::offer(payload.sdp.clone())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    // Placeholder until WebRTC pipeline is wired in Task 3
    Err(StatusCode::NOT_IMPLEMENTED)
}
```

Modify `src/lib.rs`:
```rust
use axum::{routing::{get, post}, Json, Router};
use serde_json::{json, Value};

pub mod signaling;

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test signaling_test`
Expected: PASS (empty payload gives 422, malformed SDP gives 400).

- [ ] **Step 5: Commit**

```bash
git add src/signaling.rs src/lib.rs tests/signaling_test.rs
git commit -m "feat: add signaling models and sdp offer validation"
```

---

### Task 3: WebRTC Audio Media Sink & Ogg File Recording

**Files:**
- Create: `src/recorder.rs`
- Modify: `src/signaling.rs`
- Create: `tests/recording_test.rs`

**Interfaces:**
- Consumes: `SessionDescription` from Task 2.
- Produces: `record_peer_connection(sdp_offer: String) -> Result<SessionDescription, WebRtcError>`, writing `.ogg` files to `recordings/`.

- [ ] **Step 1: Write test for Ogg audio recording sink**

Create `tests/recording_test.rs`:
```rust
use std::fs;
use std::path::Path;
use webrtc_mta_client::recorder::AudioRecorder;

#[test]
fn test_audio_recorder_creates_valid_ogg_headers() {
    let test_dir = Path::new("target/test_recordings");
    fs::create_dir_all(test_dir).unwrap();
    let test_file = test_dir.join("test_output.ogg");
    if test_file.exists() {
        let _ = fs::remove_file(&test_file);
    }

    let mut recorder = AudioRecorder::new(&test_file).expect("Failed to create audio recorder");
    recorder.close().expect("Failed to close audio recorder");

    assert!(test_file.exists(), "Ogg file was not created");
    let content = fs::read(&test_file).unwrap();
    assert!(content.len() >= 4, "File too small");
    assert_eq!(&content[0..4], b"OggS", "File missing OggS magic bytes");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test recording_test`
Expected: FAIL with "unresolved import `webrtc_mta_client::recorder`".

- [ ] **Step 3: Implement WebRTC connection engine and AudioRecorder**

Create `src/recorder.rs`:
```rust
use chrono::Local;
use rtc::interceptor::Registry;
use rtc::media::io::ogg_writer::OggWriter;
use rtc::media::io::Writer;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState, RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, channel};

pub struct AudioRecorder {
    writer: Option<OggWriter<File>>,
    path: PathBuf,
}

impl AudioRecorder {
    pub fn new(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let writer = OggWriter::new(file, 48000, 2)?;
        Ok(Self {
            writer: Some(writer),
            path: path.to_path_buf(),
        })
    }

    pub fn write_rtp(&mut self, packet: &rtc::rtp::packet::Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref mut w) = self.writer {
            w.write_rtp(packet)?;
        }
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ref mut w) = self.writer.take() {
            w.close()?;
        }
        Ok(())
    }
}

pub struct PeerHandler {
    pub runtime: Arc<dyn Runtime>,
    pub gather_complete_tx: Sender<()>,
    pub recorder: Arc<tokio::sync::Mutex<Option<AudioRecorder>>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.try_send(());
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        tracing::info!("Peer connection state: {:?}", state);
        if state == RTCPeerConnectionState::Failed || state == RTCPeerConnectionState::Closed {
            let mut guard = self.recorder.lock().await;
            if let Some(ref mut r) = *guard {
                let _ = r.close();
            }
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let media_ssrc = *track.ssrcs().await.first().unwrap_or(&0);
        let mime_type = track
            .codec(media_ssrc)
            .await
            .map(|c| c.mime_type.to_lowercase())
            .unwrap_or_default();

        if mime_type == MIME_TYPE_OPUS.to_lowercase() {
            tracing::info!("Receiving Opus audio track, writing to Ogg...");
            let recorder = Arc::clone(&self.recorder);
            self.runtime.spawn(Box::pin(async move {
                while let Some(evt) = track.poll().await {
                    if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                        let mut guard = recorder.lock().await;
                        if let Some(ref mut r) = *guard {
                            if let Err(e) = r.write_rtp(&packet) {
                                tracing::error!("Error writing RTP packet: {:?}", e);
                                break;
                            }
                        }
                    }
                }
                let mut guard = recorder.lock().await;
                if let Some(ref mut r) = *guard {
                    let _ = r.close();
                }
                tracing::info!("Audio track finished, file closed.");
            }));
        }
    }
}

pub async fn handle_webrtc_offer(
    offer_sdp: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut media_engine = MediaEngine::default();
    let audio_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            channels: 2,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
        ..Default::default()
    };
    media_engine.register_codec(audio_codec, RtpCodecKind::Audio)?;

    let config = RTCConfigurationBuilder::new().build();
    let (gather_tx, mut gather_rx) = channel::<()>(1);

    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let out_path = PathBuf::from(format!("recordings/recording_{}.ogg", ts));
    let recorder = Arc::new(tokio::sync::Mutex::new(Some(AudioRecorder::new(&out_path)?)));

    let runtime = Arc::new(webrtc::runtime::tokio::TokioRuntime::new());
    let handler = Arc::new(PeerHandler {
        runtime: runtime.clone(),
        gather_complete_tx: gather_tx,
        recorder,
    });

    let peer_connection = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_interceptor_registry(Registry::new())
        .with_handler(handler as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime.clone())
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    peer_connection
        .add_transceiver_from_kind(
            RtpCodecKind::Audio,
            Some(RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Recvonly,
                ..Default::default()
            }),
        )
        .await?;

    let offer = RTCSessionDescription::offer(offer_sdp)?;
    peer_connection.set_remote_description(offer).await?;

    let answer = peer_connection.create_answer(None).await?;
    peer_connection.set_local_description(answer).await?;

    let _ = gather_rx.recv().await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or("Failed to obtain local SDP description")?;

    Ok(local_desc.sdp)
}
```

Update `src/signaling.rs` to wire `handle_webrtc_offer`:
```rust
use axum::{http::StatusCode, Json};
use rtc::peer_connection::sdp::RTCSessionDescription;
use serde::{Deserialize, Serialize};
use crate::recorder::handle_webrtc_offer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp: String,
    pub r#type: String,
}

pub async fn offer_handler(
    Json(payload): Json<SessionDescription>,
) -> Result<Json<SessionDescription>, StatusCode> {
    if payload.r#type != "offer" {
        return Err(StatusCode::BAD_REQUEST);
    }

    RTCSessionDescription::offer(payload.sdp.clone())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match handle_webrtc_offer(payload.sdp).await {
        Ok(answer_sdp) => Ok(Json(SessionDescription {
            sdp: answer_sdp,
            r#type: "answer".to_string(),
        })),
        Err(e) => {
            tracing::error!("Failed to process WebRTC offer: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

Update `src/lib.rs` to expose `recorder`:
```rust
pub mod recorder;
pub mod signaling;
...
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test recording_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/recorder.rs src/signaling.rs src/lib.rs tests/recording_test.rs
git commit -m "feat: implement webrtc media sink and ogg opus recorder"
```

---

### Task 4: Client Web Interface & Static Asset Routing

**Files:**
- Create: `static/index.html`
- Create: `static/style.css`
- Create: `static/app.js`
- Modify: `src/lib.rs`
- Create: `tests/static_test.rs`

**Interfaces:**
- Consumes: Axum router and `/api/offer` endpoint from Task 3.
- Produces: Web interface accessible on `GET /` and `/static/*`.

- [ ] **Step 1: Write failing static route integration test**

Create `tests/static_test.rs`:
```rust
use axum::http::StatusCode;
use axum_test::TestServer;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_serves_index_html() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.get("/").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("WebRTC Audio Recorder"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test static_test`
Expected: FAIL (returns 404 Not Found).

- [ ] **Step 3: Implement frontend UI assets and static route**

Create `static/index.html`:
```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WebRTC Audio Recorder</title>
    <link rel="stylesheet" href="/static/style.css">
</head>
<body>
    <div class="card">
        <header>
            <h1>WebRTC Audio Recorder</h1>
            <p class="subtitle">Stream live microphone audio to Axum MTA ingest</p>
        </header>

        <main>
            <div class="status-container">
                <span class="status-dot" id="statusDot"></span>
                <span id="statusText">Ready</span>
            </div>

            <div class="meter-wrapper">
                <label>Audio Level</label>
                <div class="meter-bar">
                    <div class="meter-fill" id="meterFill"></div>
                </div>
            </div>

            <div class="timer" id="timer">00:00</div>

            <div class="controls">
                <button id="startBtn" class="btn btn-primary">Start Recording</button>
                <button id="stopBtn" class="btn btn-danger" disabled>Stop Recording</button>
            </div>

            <div id="message" class="message hidden"></div>
        </main>
    </div>

    <script src="/static/app.js"></script>
</body>
</html>
```

Create `static/style.css`:
```css
:root {
    --bg-color: #0f172a;
    --card-bg: #1e293b;
    --primary: #38bdf8;
    --primary-hover: #0284c7;
    --danger: #ef4444;
    --danger-hover: #dc2626;
    --text-primary: #f8fafc;
    --text-secondary: #94a3b8;
    --meter-bg: #334155;
    --meter-fill: #10b981;
}

* { box-sizing: border-box; margin: 0; padding: 0; }
body {
    background-color: var(--bg-color);
    color: var(--text-primary);
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 100vh;
    padding: 1.5rem;
}
.card {
    background-color: var(--card-bg);
    border-radius: 1rem;
    padding: 2rem;
    width: 100%;
    max-width: 440px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.4);
}
header { text-align: center; margin-bottom: 2rem; }
h1 { font-size: 1.5rem; font-weight: 700; }
.subtitle { font-size: 0.875rem; color: var(--text-secondary); margin-top: 0.25rem; }
.status-container {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
}
.status-dot {
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 50%;
    background-color: var(--text-secondary);
}
.status-dot.active { background-color: var(--meter-fill); box-shadow: 0 0 10px var(--meter-fill); }
.meter-wrapper { margin-bottom: 1.5rem; }
.meter-wrapper label { display: block; font-size: 0.75rem; color: var(--text-secondary); margin-bottom: 0.5rem; text-transform: uppercase; letter-spacing: 0.05em; }
.meter-bar { height: 0.75rem; background: var(--meter-bg); border-radius: 9999px; overflow: hidden; }
.meter-fill { height: 100%; width: 0%; background: var(--meter-fill); transition: width 0.08s ease; }
.timer { text-align: center; font-size: 2rem; font-family: monospace; font-weight: 600; margin-bottom: 1.5rem; }
.controls { display: flex; gap: 1rem; }
.btn {
    flex: 1;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border: none;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: background-color 0.2s, opacity 0.2s;
}
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background-color: var(--primary); color: #0f172a; }
.btn-primary:hover:not(:disabled) { background-color: var(--primary-hover); }
.btn-danger { background-color: var(--danger); color: white; }
.btn-danger:hover:not(:disabled) { background-color: var(--danger-hover); }
.message { margin-top: 1rem; font-size: 0.875rem; text-align: center; padding: 0.5rem; border-radius: 0.375rem; }
.message.hidden { display: none; }
```

Create `static/app.js`:
```javascript
const startBtn = document.getElementById('startBtn');
const stopBtn = document.getElementById('stopBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const meterFill = document.getElementById('meterFill');
const timerEl = document.getElementById('timer');
const messageEl = document.getElementById('message');

let localStream = null;
let peerConnection = null;
let audioContext = null;
let analyser = null;
let animFrameId = null;
let timerInterval = null;
let startTime = null;

function setStatus(text, isActive = false) {
    statusText.textContent = text;
    statusDot.classList.toggle('active', isActive);
}

function showMessage(msg, isError = false) {
    messageEl.textContent = msg;
    messageEl.classList.remove('hidden');
    messageEl.style.color = isError ? '#ef4444' : '#10b981';
}

function startTimer() {
    startTime = Date.now();
    timerInterval = setInterval(() => {
        const elapsed = Math.floor((Date.now() - startTime) / 1000);
        const mins = String(Math.floor(elapsed / 60)).padStart(2, '0');
        const secs = String(elapsed % 60).padStart(2, '0');
        timerEl.textContent = `${mins}:${secs}`;
    }, 1000);
}

function stopTimer() {
    clearInterval(timerInterval);
    timerInterval = null;
}

function startMeter(stream) {
    audioContext = new (window.AudioContext || window.webkitAudioContext)();
    const source = audioContext.createMediaStreamSource(stream);
    analyser = audioContext.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);

    const dataArray = new Uint8Array(analyser.frequencyBinCount);
    function updateMeter() {
        analyser.getByteFrequencyData(dataArray);
        const sum = dataArray.reduce((acc, val) => acc + val, 0);
        const avg = sum / dataArray.length;
        const percent = Math.min(100, Math.round((avg / 128) * 100));
        meterFill.style.width = `${percent}%`;
        animFrameId = requestAnimationFrame(updateMeter);
    }
    updateMeter();
}

function stopMeter() {
    if (animFrameId) cancelAnimationFrame(animFrameId);
    if (audioContext) {
        audioContext.close();
        audioContext = null;
    }
    meterFill.style.width = '0%';
}

startBtn.addEventListener('click', async () => {
    try {
        messageEl.classList.add('hidden');
        setStatus('Requesting microphone...');
        startBtn.disabled = true;

        localStream = await navigator.mediaDevices.getUserMedia({
            audio: { echoCancellation: true, noiseSuppression: true }
        });

        setStatus('Connecting WebRTC...');
        peerConnection = new RTCPeerConnection();
        localStream.getTracks().forEach(track => peerConnection.addTrack(track, localStream));

        startMeter(localStream);

        peerConnection.onconnectionstatechange = () => {
            if (peerConnection.connectionState === 'connected') {
                setStatus('Recording...', true);
                stopBtn.disabled = false;
                startTimer();
            } else if (peerConnection.connectionState === 'disconnected' || peerConnection.connectionState === 'closed') {
                stopRecording();
            }
        };

        const offer = await peerConnection.createOffer();
        await peerConnection.setLocalDescription(offer);

        await new Promise(resolve => {
            if (peerConnection.iceGatheringState === 'complete') {
                resolve();
            } else {
                const checkIce = () => {
                    if (peerConnection.iceGatheringState === 'complete') {
                        peerConnection.removeEventListener('icegatheringstatechange', checkIce);
                        resolve();
                    }
                };
                peerConnection.addEventListener('icegatheringstatechange', checkIce);
            }
        });

        const res = await fetch('/api/offer', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                sdp: peerConnection.localDescription.sdp,
                type: peerConnection.localDescription.type
            })
        });

        if (!res.ok) {
            throw new Error(`Signaling failed: ${res.statusText}`);
        }

        const answer = await res.json();
        await peerConnection.setRemoteDescription(new RTCSessionDescription(answer));

    } catch (err) {
        showMessage(err.message, true);
        stopRecording();
    }
});

function stopRecording() {
    stopTimer();
    stopMeter();
    if (localStream) {
        localStream.getTracks().forEach(t => t.stop());
        localStream = null;
    }
    if (peerConnection) {
        peerConnection.close();
        peerConnection = null;
    }
    startBtn.disabled = false;
    stopBtn.disabled = true;
    setStatus('Ready', false);
    showMessage('Recording stopped and saved on server.');
}

stopBtn.addEventListener('click', stopRecording);
```

Modify `src/lib.rs` to serve static files:
```rust
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

pub mod recorder;
pub mod signaling;

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(ServeFile::new("static/index.html"))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test static_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add static/ src/lib.rs tests/static_test.rs
git commit -m "feat: add client web ui and static asset serving"
```

---

### Task 5: End-to-End Live Integration & Verification

**Files:**
- Create: `tests/e2e_test.rs`
- Verify: Full test suite passing via `cargo test`

**Interfaces:**
- Consumes: All components from Tasks 1-4.
- Produces: Verified working application ready for run & recording.

- [ ] **Step 1: Write integration loopback test**

Create `tests/e2e_test.rs`:
```rust
use axum_test::TestServer;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_full_system_smoke() {
    let server = TestServer::new(app().await).unwrap();

    let health = server.get("/api/health").await;
    assert_eq!(health.status_code(), 200);

    let index = server.get("/").await;
    assert_eq!(index.status_code(), 200);

    let script = server.get("/static/app.js").await;
    assert_eq!(script.status_code(), 200);
}
```

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass (0 failures).

- [ ] **Step 3: Verification build & check**

Run: `cargo check`
Expected: Success with no warnings or errors.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e_test.rs
git commit -m "test: add end-to-end smoke tests and verify build"
```
