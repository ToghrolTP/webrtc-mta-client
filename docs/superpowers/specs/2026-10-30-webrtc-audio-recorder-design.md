# WebRTC Audio Ingest & Recording Service Specification

## 1. Overview
The goal of this project is to create a web application that captures user microphone audio in a browser via WebRTC, transmits the audio stream to a Rust Axum backend via a WebRTC `RTCPeerConnection`, and records the incoming audio into an Ogg/Opus file on disk. The system is architected as an initial ingest stage that will subsequently allow redirecting or forwarding the live audio stream to an external MTA (Media Transport / Transfer Agent) server.

## 2. Architecture & Components

```
+-----------------------------------+
|          Web Browser UI           |
| (HTML5 + Web Audio API + WebRTC)  |
+-----------------+-----------------+
                  |
    1. HTTP POST  | 2. WebRTC PeerConnection
    /api/offer    |    (RTP / SRTP - Opus)
                  v
+-----------------+-----------------+
|         Axum Web Server           |
|  - Static File Serving (/)        |
|  - Signaling Endpoint (/api/offer)|
|  - Health Endpoint (/api/health)  |
+-----------------+-----------------+
                  |
                  v
+-----------------+-----------------+
|     webrtc-rs Engine Runtime      |
|  - MediaEngine (Opus 48kHz, 2ch)  |
|  - Transceiver (Recvonly)         |
|  - on_track RTP packet demuxer    |
+-----------------+-----------------+
                  |
                  v
+-----------------+-----------------+
|         OggWriter Sink            |
| - Writes recordings/rec_<ts>.ogg  |
| - Finalizes Ogg Opus headers      |
+-----------------------------------+
```

### 2.1 Backend (`webrtc-mta-client`)
* **Framework**: Axum web framework running on Tokio.
* **WebRTC Stack**: Official `webrtc` crate (`webrtc.rs`).
* **Storage**: Local filesystem directory `recordings/` storing `.ogg` container files with Opus payload.
* **Dependencies**:
  * `axum` (0.8 or compatible): HTTP server and routing.
  * `tokio` (1.x): Asynchronous runtime.
  * `tower-http`: `ServeDir` for serving static assets, CORS, and request tracing.
  * `webrtc` (0.12 or compatible): WebRTC peer connection, SDP, RTP demuxing, and `webrtc::media::io::ogg_writer::OggWriter`.
  * `serde` and `serde_json`: Serialization for signaling messages.
  * `tracing` and `tracing-subscriber`: Structured logging.
  * `chrono`: Timestamp generation for recording filenames.

### 2.2 Frontend Client (`static/`)
* **Zero-build**: Pure modern vanilla HTML5, CSS3, and ES6 JavaScript.
* **Capabilities**:
  * Microphone capture via `navigator.mediaDevices.getUserMedia`.
  * Real-time audio meter using Web Audio API (`AudioContext` and `AnalyserNode`).
  * WebRTC peer connection creation and audio track transmission.
  * Single-roundtrip HTTP POST signaling.

## 3. Data Flow & Protocols

### 3.1 Signaling Protocol
* **Endpoint**: `POST /api/offer`
* **Request Payload**:
  ```json
  {
    "sdp": "v=0\r\no=- 0 0 IN IP4 127.0.0.1...",
    "type": "offer"
  }
  ```
* **Response Payload**:
  ```json
  {
    "sdp": "v=0\r\no=- 0 0 IN IP4 127.0.0.1...",
    "type": "answer"
  }
  ```
* **Flow**:
  1. Browser initializes `RTCPeerConnection` with standard configuration.
  2. Browser calls `navigator.mediaDevices.getUserMedia({ audio: true })` and attaches the audio track to the peer connection via `pc.addTrack(track, stream)`.
  3. Browser calls `pc.createOffer()` and `pc.setLocalDescription(offer)`.
  4. Browser waits until `pc.iceGatheringState === 'complete'` so all host candidates are embedded in the SDP offer.
  5. Browser sends `POST /api/offer` with the complete SDP offer.
  6. Backend deserializes SDP offer, sets it as remote description on a newly created `RTCPeerConnection`.
  7. Backend creates answer, sets local description, waits for ICE gathering complete, and returns the SDP answer in the HTTP response.
  8. Browser receives SDP answer and executes `pc.setRemoteDescription(answer)`. WebRTC media connection establishes.

### 3.2 Media Recording Pipeline
1. In Axum's WebRTC handler, an audio transceiver is set up with direction `Recvonly`.
2. When the peer connection receives the audio track, the `on_track` callback is invoked with `Arc<TrackRemote>`.
3. A dedicated Tokio task is spawned:
   * A timestamped file is opened: `recordings/recording_<YYYYMMDD_HHMMSS>.ogg`.
   * An `OggWriter` is created with clock rate `48000` and channel count `2`.
   * The task continuously reads RTP packets via `track.read_rtp().await`.
   * For each packet, `ogg_writer.write_rtp(&packet)` writes the Opus frames into the Ogg container.
   * Upon stream termination (stream error, EOF, or peer disconnect), `ogg_writer.close()` writes end-of-stream pages.

### 3.3 HTTP Endpoints
* `GET /`: Serves `static/index.html`.
* `GET /static/*`: Serves client JavaScript, CSS, and assets.
* `POST /api/offer`: WebRTC SDP exchange.
* `GET /api/health`: Returns HTTP 200 `{"status": "ok"}` for health probes.

## 4. Error Handling & Edge Cases
* **Missing Microphone Permissions**: Frontend displays an informative error banner without crashing.
* **ICE Connection Failure / Timeout**: Backend cleans up peer connection resources; frontend indicates connection failure and resets UI state.
* **Malformed Signaling Payloads**: `POST /api/offer` validates JSON structure and returns `400 Bad Request` on invalid SDP.
* **Abrupt Disconnections**: Backend handles `PeerConnectionState::Failed` or `PeerConnectionState::Disconnected` by finalizing the current Ogg file.
* **Missing Recordings Folder**: Backend ensures `recordings/` directory exists on startup.

## 5. Verification & Testing Strategy
* **Unit/Integration Tests (`tests/`)**:
  * Health check test verifying `GET /api/health` returns status 200.
  * Signaling validation test verifying `POST /api/offer` rejects malformed payloads with 400.
  * Synthetic WebRTC loopback/mock test creating a peer connection offer and validating answer generation.
* **Recording Integrity Test**:
  * Automated test validating that writing RTP Opus frames generates a non-empty `.ogg` file starting with `OggS` and containing the `OpusHead` identification header.
* **Manual Live Check**:
  * Run the server on `127.0.0.1:3000`, open the UI in a browser, record 5 seconds of audio, verify `.ogg` file creation, and play back or inspect audio metadata.
