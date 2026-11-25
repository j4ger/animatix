use super::native::NativeLoader;
use super::{decode::Decoder, probe};
use animatix::timeline::video::VideoLoader;

#[test]
fn variable_rate_and_rotated_clips_seek_to_the_same_source_frame() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("FFmpeg fixture generator unavailable; skipping native video decode fixture");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("input.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=8:duration=4",
            "-vf",
            "select='not(eq(mod(n,5),2))'",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "8",
            "-threads",
            "1",
        ])
        .arg(&original)
        .status()
        .unwrap();
    assert!(status.success());
    let rotated = directory.path().join("rotated.mov");
    let status = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-display_rotation:v:0", "90", "-i"])
        .arg(original)
        .args(["-c", "copy"])
        .arg(&rotated)
        .status()
        .unwrap();
    assert!(status.success());
    let media = probe::read(&rotated).unwrap();
    assert_eq!(media.metadata.display_size, [48, 64]);

    let mut sequential = Decoder::open(&rotated, media, 0.0).unwrap();
    // 0.25 s is a missing frame. Its pixels must still be the frame at 0.125 s.
    let previous = sequential.sample(0.125).unwrap().unwrap();
    let gap = sequential.sample(0.25).unwrap().unwrap();
    assert_eq!(previous.data.data.as_ref(), gap.data.data.as_ref());
    let changed = sequential.sample(0.375).unwrap().unwrap();
    assert_ne!(previous.data.data.as_ref(), changed.data.data.as_ref());

    let file = NativeLoader::default().open(&rotated).unwrap();
    let times = [0.51, 1.24, 1.25, 2.78, 3.71];
    let expected = times.map(|time| sequential.sample(time).unwrap().unwrap());
    for (time, expected) in times.into_iter().zip(expected).rev() {
        let sought = file.frame_at(time).unwrap().unwrap();
        assert_eq!(expected.data.data.as_ref(), sought.data.data.as_ref(), "{time}s");
    }
}
