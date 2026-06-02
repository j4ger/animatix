use animatix::timeline::{Interpolate, PropertyTrack};
use animatix_syntax::easing::Easing;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_property_interpolation(c: &mut Criterion) {
    let mut track = PropertyTrack::new(0.0f32);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 100.0, Easing::EaseInOut);
    track.add_keyframe(2000, 50.0, Easing::EaseOut);

    c.bench_function("property_track_evaluate", |b| {
        b.iter(|| {
            black_box(track.evaluate(black_box(500)));
        })
    });

    c.bench_function("interpolate_f32", |b| {
        b.iter(|| {
            black_box(0.0f32.interpolate(&black_box(100.0), black_box(0.5)));
        })
    });

    c.bench_function("interpolate_vec4", |b| {
        let a = [0.0f32, 0.0, 0.0, 1.0];
        let z = [1.0f32, 1.0, 1.0, 1.0];
        b.iter(|| {
            black_box(a.interpolate(&black_box(z), black_box(0.5)));
        })
    });
}

criterion_group!(benches, bench_property_interpolation);
criterion_main!(benches);
