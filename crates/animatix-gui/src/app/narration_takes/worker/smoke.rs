use super::*;
use crate::app::narration_takes::bindings;

struct Project {
    folder: PathBuf,
    history: Arc<History>,
    python: OsString,
}

impl Project {
    fn apply(
        &self,
        bindings: &[Binding],
        index: usize,
        operation: Operation,
        id: &str,
        base: Option<Pair>,
    ) -> Result<Finished, String> {
        let binding = &bindings[index];
        let kind = if matches!(&operation, Operation::Synthesize { .. }) {
            "synthesized"
        } else {
            "imported"
        };
        let request = Request {
            folder: self.folder.clone(),
            name: "Intro".into(),
            paragraphs: bindings
                .iter()
                .map(|binding| ManifestParagraph {
                    id: binding.selector.clone(),
                    text: binding.text.clone(),
                })
                .collect(),
            bindings: bindings.to_vec(),
            base,
            python: self.python.clone(),
            operation,
            take: Take {
                id: id.into(),
                paragraph: binding.id.clone(),
                scene: None,
                text: binding.text.clone(),
                kind,
            },
            selector: binding.selector.clone(),
            history: Arc::clone(&self.history),
        };
        run(request, &AtomicBool::new(false))
    }
}

fn fixture(folder: &Path, text: &str, samples: usize) -> Operation {
    let wave = folder.join("fixture.wav");
    let timing = folder.join("fixture.speech.json");
    let mut out = hound::WavWriter::create(
        &wave,
        hound::WavSpec {
            channels: 1,
            sample_rate: 24_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..samples {
        out.write_sample(42i16).unwrap();
    }
    out.finalize().unwrap();
    fs::write(
        &timing,
        serde_json::to_vec(&serde_json::json!([{
            "word": text, "start": 0.01, "end": 0.08
        }]))
        .unwrap(),
    )
    .unwrap();
    Operation::Import {
        audio: Some(wave),
        timing: ImportTiming::Sidecar(timing),
    }
}

fn pair(output: Finished) -> Pair {
    (output.adopted.audio, output.adopted.transcript)
}

#[test]
#[ignore = "requires Python 3 on PATH"]
fn bundled_worker_splices_reopened_history_and_keeps_the_previous_pair() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().canonicalize().unwrap();
    let source = folder.join("demo.amx");
    let project = Project {
        folder: folder.clone(),
        history: Arc::new(History::open(&source, true).unwrap().unwrap()),
        python: OsString::from(if cfg!(windows) { "python" } else { "python3" }),
    };
    let before = "/// Hello.\n\n/// Again.\n";
    let bindings = bindings::reconcile(before, &[]);
    let first = pair(
        project
            .apply(
                &bindings,
                0,
                fixture(&folder, "Hello.", 3_000),
                "10000000000000000000000000000000",
                None,
            )
            .unwrap(),
    );
    let second = pair(
        project
            .apply(
                &bindings,
                1,
                fixture(&folder, "Again.", 3_500),
                "20000000000000000000000000000000",
                Some(first.clone()),
            )
            .unwrap(),
    );
    project.history.snapshot(before, &bindings, true).unwrap();
    drop(project);

    let after = "/// Welcome.\n\n/// Again.\n";
    let project = Project {
        folder: folder.clone(),
        history: Arc::new(History::open(&source, false).unwrap().unwrap()),
        python: OsString::from(if cfg!(windows) { "python" } else { "python3" }),
    };
    let (_, previous) = project.history.bindings(before).unwrap();
    let bindings = bindings::edit(before, after, &previous);
    let third = pair(
        project
            .apply(
                &bindings,
                0,
                fixture(&folder, "Welcome.", 4_000),
                "30000000000000000000000000000000",
                Some(second.clone()),
            )
            .unwrap(),
    );
    assert_eq!(project.history.takes().unwrap().len(), 3);
    assert_ne!(third, second);
    assert!(first.0.is_file() && second.0.is_file());
    assert!(!folder.join(".animatix-narration").exists());
    let read = |path: &Path| {
        let text = fs::read_to_string(path).unwrap();
        animatix::narration::parse_transcript_auto(&text).unwrap()
    };
    let previous = read(&second.1);
    let updated = read(&third.1);
    assert_eq!(
        updated.segments.iter().map(|segment| segment.text.as_str()).collect::<Vec<_>>(),
        ["Welcome.", "Again."]
    );
    assert!(updated.segments[1].start_s > previous.segments[1].start_s);
    assert_eq!(third.0.parent(), Some(folder.join("narration").as_path()));

    let copy = tempfile::tempdir().unwrap();
    let relative = (third.0.strip_prefix(&folder).unwrap(), third.1.strip_prefix(&folder).unwrap());
    let portable = (copy.path().join(relative.0), copy.path().join(relative.1));
    assets::copy_pair(&third, &portable).unwrap();
    assert_eq!(read(&portable.1), updated);
    assert_eq!(assets::fingerprint(&portable).unwrap(), assets::fingerprint(&third).unwrap());
}

#[test]
fn missing_alignment_keeps_imported_audio() {
    let directory = tempfile::tempdir().unwrap();
    let folder = directory.path().canonicalize().unwrap();
    let project = Project {
        folder: folder.clone(),
        history: Arc::new(History::open(&folder.join("demo.amx"), true).unwrap().unwrap()),
        python: OsString::from("unused"),
    };
    let bindings = bindings::reconcile("/// Hello.\n", &[]);
    let Operation::Import { audio, .. } = fixture(&folder, "Hello.", 3_000) else {
        unreachable!()
    };
    let result = project.apply(
        &bindings,
        0,
        Operation::Import {
            audio,
            timing: ImportTiming::Missing,
        },
        "10000000000000000000000000000000",
        None,
    );
    assert!(result.is_err());
    let takes = project.history.takes().unwrap();
    assert_eq!(takes.len(), 1);
    assert!(takes[0].issue.is_some() && !takes[0].aligned);
    assert!(!folder.join("narration").exists());
}
