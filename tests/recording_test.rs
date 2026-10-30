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
    assert!(
        content.windows(8).any(|w| w == b"OpusHead"),
        "File missing OpusHead header"
    );
}
