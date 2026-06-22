use super::*;

#[test]
fn test_container_metadata_gap_helpers() {
    // gap_uniform creates a uniform [f32; 2] from a scalar
    let g = gap_uniform(10.0);
    assert_eq!(g, [10.0, 10.0]);

    // padding_uniform creates a uniform [f32; 4] from a scalar
    let p = padding_uniform(8.0);
    assert_eq!(p, [8.0, 8.0, 8.0, 8.0]);
}
