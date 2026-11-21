use std::cell::RefCell;

use super::interpolate::AlignedPath;
use super::{BezPath, MorphOptions, MorphStrategy, PathEl, Point, align_path_lists_with_strategy};

/// Scratch memo for the last pair of path-track endpoint outlines.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct AlignmentCache(RefCell<Option<Box<CachedAlignment>>>);

impl Clone for AlignmentCache {
    fn clone(&self) -> Self {
        // A cloned actor starts with its own empty render cache.
        Self::default()
    }
}

#[derive(Debug)]
struct CachedAlignment {
    source: Vec<BezPath>,
    target: Vec<BezPath>,
    strategy: MorphStrategy,
    stretch: bool,
    aligned: Vec<AlignedPath>,
}

pub(super) fn with_alignment<P, R>(
    cache: Option<&AlignmentCache>,
    source: &[P],
    target: &[P],
    outline: impl Fn(&P) -> &BezPath,
    options: MorphOptions,
    sample: impl FnOnce(&[AlignedPath]) -> R,
) -> R {
    let mut last = cache.map(|cache| cache.0.borrow_mut());
    if let Some(entry) = last.as_deref().and_then(Option::as_ref)
        && entry.strategy == options.strategy
        && entry.stretch == options.stretch
        && same_outlines(&entry.source, source, &outline)
        && same_outlines(&entry.target, target, &outline)
    {
        return sample(&entry.aligned);
    }

    let source: Vec<_> = source.iter().map(|path| outline(path).clone()).collect();
    let target: Vec<_> = target.iter().map(|path| outline(path).clone()).collect();
    let aligned = align(&source, &target, options);
    let output = sample(&aligned);
    if let Some(last) = last.as_deref_mut() {
        *last = Some(Box::new(CachedAlignment {
            source,
            target,
            strategy: options.strategy,
            stretch: options.stretch,
            aligned,
        }));
    }
    output
}

fn align(source: &[BezPath], target: &[BezPath], options: MorphOptions) -> Vec<AlignedPath> {
    align_path_lists_with_strategy(source, target, options.strategy)
        .into_iter()
        .map(|(source, target)| AlignedPath::new(&source, &target, options))
        .collect()
}

fn same_outlines<P>(cached: &[BezPath], paths: &[P], outline: &impl Fn(&P) -> &BezPath) -> bool {
    // Track data can be edited in place. Verify the original geometry rather
    // than relying on a time or allocation address remaining unique after an edit.
    cached.len() == paths.len()
        && cached.iter().zip(paths).all(|(cached, path)| {
            let a = cached.elements();
            let b = outline(path).elements();
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_element(a, b))
        })
}

fn same_element(a: &PathEl, b: &PathEl) -> bool {
    fn point(a: Point, b: Point) -> bool {
        a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits()
    }

    match (*a, *b) {
        (PathEl::MoveTo(a), PathEl::MoveTo(b)) | (PathEl::LineTo(a), PathEl::LineTo(b)) => {
            point(a, b)
        },
        (PathEl::QuadTo(a, b), PathEl::QuadTo(c, d)) => point(a, c) && point(b, d),
        (PathEl::CurveTo(a, b, c), PathEl::CurveTo(d, e, f)) => {
            point(a, d) && point(b, e) && point(c, f)
        },
        (PathEl::ClosePath, PathEl::ClosePath) => true,
        _ => false, // Different segment kinds require fresh alignment.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_interpolation_arc_reuses_alignment() {
        let cache = AlignmentCache::default();
        let source = vec![BezPath::from_vec(vec![PathEl::MoveTo(Point::ZERO)])];
        let target = vec![BezPath::from_vec(vec![PathEl::MoveTo(Point::new(
            10.0, 10.0,
        ))])];
        let prepare = |options| {
            with_alignment(
                Some(&cache),
                &source,
                &target,
                |path| path,
                options,
                |paths| paths.as_ptr(),
            )
        };
        let start = prepare(MorphOptions::default());
        assert_eq!(
            start,
            prepare(MorphOptions {
                path_arc: 0.5,
                ..Default::default()
            })
        );
        assert_ne!(
            start,
            prepare(MorphOptions {
                stretch: true,
                ..Default::default()
            })
        );
        assert!(cache.clone().0.borrow().is_none());
    }
}
