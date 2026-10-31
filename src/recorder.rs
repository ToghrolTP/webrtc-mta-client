use chrono::Local;
use rtc::interceptor::Registry;
use rtc::media::io::Writer;
use rtc::media::io::ogg_writer::OggWriter;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use webrtc::media_stream::track_remote::{TrackRemote, TrackRemoteEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
    RTCPeerConnectionState,
};
use webrtc::runtime::{Runtime, Sender, TokioRuntime, channel};

pub struct AudioRecorder {
    writer: Option<OggWriter<File>>,
    path: PathBuf,
}

impl AudioRecorder {
    pub fn new(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let writer = OggWriter::new(file, 48000, 2)?;
        Ok(Self {
            writer: Some(writer),
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write_rtp(
        &mut self,
        packet: &rtc::rtp::packet::Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    pub recordings_dir: PathBuf,
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
        if state == RTCPeerConnectionState::Disconnected
            || state == RTCPeerConnectionState::Failed
            || state == RTCPeerConnectionState::Closed
        {
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
            let ts = Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
            let out_path = self.recordings_dir.join(format!("recording_{}.ogg", ts));

            match AudioRecorder::new(&out_path) {
                Ok(rec) => {
                    let mut guard = self.recorder.lock().await;
                    if let Some(ref mut old) = *guard {
                        let _ = old.close();
                    }
                    *guard = Some(rec);
                }
                Err(e) => {
                    tracing::error!("Failed to create audio recorder at {:?}: {:?}", out_path, e);
                    return;
                }
            }

            let recorder = Arc::clone(&self.recorder);
            self.runtime.spawn(Box::pin(async move {
                while let Some(evt) = track.poll().await {
                    if let TrackRemoteEvent::OnRtpPacket(packet) = evt {
                        let mut guard = recorder.lock().await;
                        if let Some(ref mut r) = *guard
                            && let Err(e) = r.write_rtp(&packet)
                        {
                            tracing::error!("Error writing RTP packet: {:?}", e);
                            break;
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
    };
    media_engine.register_codec(audio_codec, RtpCodecKind::Audio)?;

    let config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![
                "stun:stun.l.google.com:19302".to_string(),
                "stun:stun1.l.google.com:19302".to_string(),
            ],
            ..Default::default()
        }])
        .build();
    let (gather_tx, mut gather_rx) = channel::<()>(1);

    let recorder = Arc::new(tokio::sync::Mutex::new(None));
    let recordings_dir = PathBuf::from("recordings");

    let runtime = Arc::new(TokioRuntime);
    let handler = Arc::new(PeerHandler {
        runtime: runtime.clone(),
        gather_complete_tx: gather_tx,
        recorder,
        recordings_dir,
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

    let _ = timeout(Duration::from_secs(5), gather_rx.recv()).await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or("Failed to obtain local SDP description")?;

    Ok(local_desc.sdp)
}
