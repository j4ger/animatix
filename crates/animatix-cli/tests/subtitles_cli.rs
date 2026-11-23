use std::{fs, process::Command};

#[test]
fn export_and_correct_subtitles_without_a_renderer() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::write(root.join("raw.json"), r#"{"segments":[{"id":7,"start":0.3,"end":2.75,"text":"helo","words":[{"word":"helo","start":0.3,"end":2.75}]}]}"#).unwrap();
    fs::write(
        root.join("edits.json"),
        r#"[{"id":7,"text":"hello","words":[{"index":0,"word":"hello","start_s":0.4}]}]"#,
    )
    .unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_animatix"))
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    };
    run(&[
        "transcript",
        "raw.json",
        "--edits",
        "edits.json",
        "-o",
        "words.speech.json",
    ]);
    let corrected: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("words.speech.json")).unwrap()).unwrap();
    assert_eq!(corrected["segments"][0]["words"][0]["word"], "hello");
    assert_eq!(corrected["segments"][0]["words"][0]["start_s"], 0.4);

    fs::write(
        root.join("scene.amx"),
        r#"
config { duration: 2 }
#0s
subs: Captions, transcript: "words.speech.json"
"#,
    )
    .unwrap();
    run(&[
        "subtitles",
        "scene.amx",
        "--track",
        "subs",
        "-o",
        "words.srt",
    ]);
    assert_eq!(
        fs::read_to_string(root.join("words.srt")).unwrap(),
        "1\n00:00:00,300 --> 00:00:02,000\nhello\n\n"
    );
    run(&["subtitles", "scene.amx", "-o", "words.vtt"]);
    assert_eq!(
        fs::read_to_string(root.join("words.vtt")).unwrap(),
        "WEBVTT\n\n00:00:00.300 --> 00:00:02.000\nhello\n\n"
    );
}
