use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_review_run() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "animatix-review-test-{}-{}",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&path).expect("create temp review run");
    fs::write(path.join("brief.md"), "Five cards enter with a stagger.").expect("write temp brief");
    fs::write(
        path.join("a.amx"),
        r#"config { colorscheme: "editorial-dark", resolution: (640, 360) }
a: Rect, size: (100, 100), color: accent.primary
"#,
    )
    .expect("write temp variant a");
    fs::write(
        path.join("b.amx"),
        r#"config { colorscheme: "editorial-dark", resolution: (640, 360) }
b: Rect, size: (100, 100), color: accent.primary
"#,
    )
    .expect("write temp variant b");
    path
}

#[test]
fn review_run_loads_variants_from_local_directory() {
    let path = temp_review_run();
    let run = RunLoader::load(&path).expect("temp review run should load");
    let ids: Vec<&str> = run.variants.iter().map(|variant| variant.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
    assert_eq!(run.variants.len(), 2);
    fs::remove_dir_all(path).expect("remove temp review run");
}

#[test]
fn comments_roundtrip_through_json() {
    let comments = vec![
        ReviewComment {
            id: "c1".to_string(),
            variant: "a".to_string(),
            time_ms: Some(420),
            severity: CommentSeverity::Major,
            note: "stagger feels more direct".to_string(),
        },
        ReviewComment {
            id: "c2".to_string(),
            variant: "b".to_string(),
            time_ms: None,
            severity: CommentSeverity::Question,
            note: "do we need the explicit delays?".to_string(),
        },
    ];

    let json = serde_json::to_string(&ReviewComments {
        comments: comments.clone(),
    })
    .expect("serialize comments");

    let parsed: ReviewComments = serde_json::from_str(&json).expect("deserialize comments");
    assert_eq!(parsed.comments, comments);
}
