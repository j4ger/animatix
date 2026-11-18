//! Native chapter planning with imported source-relative media.

#![cfg(feature = "video")]

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

const ENTRY: &str = r#"
import "lesson/chapter.amx" as lesson
config { resolution: (160, 90) }

# First
config { duration: 0.75 }
play lesson::Middle [fade, 250ms]

# Last
config { duration: 0.75 }
"#;

const CHAPTER: &str = r#"
# Middle
config { duration: 0.75 }
voice: Audio, source: "voice.wav", transcript: "speech/voice.speech.json"
caption: Captions, transcript: "speech/captions.speech.json", opacity: 1
square: Rect, size: (20, 20), color: red
#voice@beat
  square.opacity = 1
play Last [fade, 250ms]
"#;

const TRANSCRIPT: &str = r#"{
    "duration": 0.5,
    "markers": {"beat": 0.2},
    "segments": [{"start": 0.2, "end": 0.5, "text": "A cue"}]
}"#;

#[test]
fn chapter_plan_tracks_imported_scene_source_audio_and_both_transcripts() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    let entry = root.join("main.amx");
    let chapter = root.join("lesson/chapter.amx");
    let speech = root.join("lesson/speech");
    fs::create_dir_all(&speech).unwrap();
    fs::write(&entry, ENTRY).unwrap();
    fs::write(&chapter, CHAPTER).unwrap();
    fs::write(speech.join("voice.speech.json"), TRANSCRIPT).unwrap();
    fs::write(speech.join("captions.speech.json"), TRANSCRIPT).unwrap();

    let before = plan(&entry);
    assert_eq!(before["width"], 160);
    assert_eq!(before["height"], 90);
    assert!(before["files"].get(chapter.to_str().unwrap()).is_some());
    assert_eq!(
        before["scenes"]["lesson::Middle"]["assets"],
        json!([
            speech.join("captions.speech.json"),
            speech.join("voice.speech.json"),
            root.join("lesson/voice.wav"),
        ])
    );
    assert_eq!(before["scenes"]["First"]["assets"], json!([]));
    assert_eq!(before["audio"][0]["source"], json!(root.join("lesson/voice.wav")));
    assert_eq!(before["chapters"][0]["scenes"], json!(["First"]));
    assert_eq!(before["chapters"][1]["scenes"], json!(["First", "lesson::Middle"]));
    assert_eq!(before["chapters"][2]["scenes"], json!(["Last", "lesson::Middle"]));

    fs::write(&chapter, CHAPTER.replace("color: red", "color: blue")).unwrap();
    let after = plan(&entry);
    assert_eq!(before["shared_source"], after["shared_source"]);
    assert_eq!(before["scenes"]["First"], after["scenes"]["First"]);
    assert_eq!(before["scenes"]["Last"], after["scenes"]["Last"]);
    assert_ne!(
        before["scenes"]["lesson::Middle"]["source"],
        after["scenes"]["lesson::Middle"]["source"]
    );
}

fn plan(path: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .args(["--no-color", "chapter-plan"])
        .arg(path)
        .args(["--fps", "10", "--hold", "0"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
