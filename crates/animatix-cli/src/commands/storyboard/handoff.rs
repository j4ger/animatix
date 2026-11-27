use animatix::composition::BuildTarget;
use animatix::time::{add_seconds, scene_local_seconds};

pub(super) fn sample_times(target: &BuildTarget, from: &str) -> Result<Vec<f64>, String> {
    let BuildTarget::MultiScene(comp) = target else {
        return Err("--transition-from requires a multi-scene composition".into());
    };
    let order = comp.playback_order();
    let index = order
        .iter()
        .position(|name| name == from)
        .ok_or_else(|| format!("Scene '{from}' is absent from playback"))?;
    let to = order
        .get(index + 1)
        .ok_or_else(|| format!("Scene '{from}' has no following scene"))?;
    let from_start = comp.scene_start_times[from];
    let start = comp.scene_start_times[to];
    // A short incoming scene can start another handoff before this one finishes.
    // Playback composites the latest two active scenes.
    let next_start = order.get(index + 2).map(|name| comp.scene_start_times[name]);
    let end = add_seconds(from_start, comp.scenes[from].duration_s)
        .min(next_start.unwrap_or(comp.global_duration_s));
    // One storyboard timestamp unit (0.01 s) before the overlap, when available.
    let before = add_seconds(start, -0.01).max(from_start);
    let mut times = vec![before];
    if start < end {
        times.push(add_seconds(start, scene_local_seconds(end, start) / 2.0));
    }
    times.push(end);
    times.dedup();
    Ok(times)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_the_scheduled_overlap_and_both_sides_of_an_implicit_cut() {
        let source = r#"
# First
config { duration: 2 }
#0s
a: Rect
play Second [fade, 400ms]
# Second
config { duration: 1 }
#0s
b: Rect
# Third
config { duration: 1 }
#0s
c: Rect
"#;
        let target = build(source);
        assert_eq!(sample_times(&target, "First").unwrap(), [1.59, 1.8, 2.0]);
        assert_eq!(sample_times(&target, "Second").unwrap(), [2.59, 2.6]);
        assert!(sample_times(&target, "Third").is_err());
        assert!(sample_times(&target, "Missing").is_err());

        let interrupted = build(&source.replace("b: Rect", "b: Rect\nplay Third [fade, 800ms]"));
        assert_eq!(sample_times(&interrupted, "First").unwrap(), [1.59, 1.7, 1.8]);
    }

    fn build(source: &str) -> BuildTarget {
        let ast = animatix_syntax::parser::parse_source(source).0.unwrap();
        BuildTarget::build_with_options(&ast, &Default::default(), None, Default::default()).output
    }
}
