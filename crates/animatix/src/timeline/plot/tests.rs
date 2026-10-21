use super::adaptive::{base_subdivision_depth, math_to_screen_padded};

/// With zero padding the formula degenerates to the original:
/// `screen = (norm - 0.5) * size`
#[test]
fn math_to_screen_padded_zero_padding_matches_original() {
    let x_domain = [-1.0_f64, 1.0];
    let y_domain = [-1.0_f64, 1.0];
    let p_size = [200.0_f64, 200.0];
    let padding = [0.0_f64; 4];

    // domain center → screen (0, 0)
    let (sx, sy) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sx).abs() < 1e-10, "expected sx=0, got {sx}");
    assert!((sy).abs() < 1e-10, "expected sy=0, got {sy}");

    // domain max-x → screen x = +100  (half of 200)
    let (sx, _) = math_to_screen_padded(1.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sx - 100.0).abs() < 1e-10, "expected sx=100, got {sx}");

    // domain min-y → screen y = +100  (screen Y increases downward)
    let (_, sy) = math_to_screen_padded(0.0, -1.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sy - 100.0).abs() < 1e-10, "expected sy=100, got {sy}");
}

/// Symmetric padding shrinks the plot area proportionally.
/// With [p, p, p, p] padding the centre remains at screen (0, 0) and
/// the domain boundary maps to (half_size - p) instead of half_size.
#[test]
fn math_to_screen_padded_symmetric_padding_shrinks_area() {
    let x_domain = [-1.0_f64, 1.0];
    let y_domain = [-1.0_f64, 1.0];
    let p_size = [200.0_f64, 200.0];
    let padding = [10.0_f64; 4]; // 10 px on every side

    // domain centre still maps to screen centre
    let (sx, sy) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sx).abs() < 1e-10, "centre sx should be 0, got {sx}");
    assert!((sy).abs() < 1e-10, "centre sy should be 0, got {sy}");

    // domain max-x maps to (plot_w / 2) = (200 - 10 - 10) / 2 = 90
    let (sx, _) = math_to_screen_padded(1.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sx - 90.0).abs() < 1e-10, "expected sx=90, got {sx}");
}

/// Asymmetric left/right padding shifts the plot center in x.
#[test]
fn math_to_screen_padded_asymmetric_padding_shifts_center() {
    let x_domain = [-1.0_f64, 1.0];
    let y_domain = [-1.0_f64, 1.0];
    let p_size = [200.0_f64, 200.0];
    // 20 px on the left, 0 on the right → center shifts right by 10
    let padding = [20.0, 0.0, 0.0, 0.0];

    // domain centre maps to x = (20 - 0) / 2 = 10
    let (sx, _) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
    assert!((sx - 10.0).abs() < 1e-10, "expected sx=10 with left-only padding, got {sx}");
}

#[test]
fn resolution_is_a_sample_count_not_a_depth() {
    // `resolution` asks for base *segments*; the sampler wants a subdivision
    // *depth*. Clamping the sample count by the depth cap before the log2 is
    // what made every value above `max_depth` render identically.
    assert_eq!(base_subdivision_depth(16, 10), 4);
    assert_eq!(base_subdivision_depth(96, 10), 6);
    assert_eq!(base_subdivision_depth(256, 10), 8, "log2 of the sample count");
    // The cap still bites — but only once the *depth* exceeds it, which is
    // where the old expression clamped the sample count instead and lost
    // every request above `max_depth` outright.
    assert_eq!(base_subdivision_depth(4096, 10), 10);
    assert_eq!(base_subdivision_depth(1024, 12), 10);
}

#[test]
fn tiny_and_bogus_resolutions_stay_in_range() {
    assert_eq!(base_subdivision_depth(0, 10), 1);
    assert_eq!(base_subdivision_depth(1, 10), 1);
    assert_eq!(base_subdivision_depth(2, 0), 0);
}
