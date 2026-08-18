use animatix::timeline::{
    ActorKindId, Interpolate, PropertyPlan, PropertyTrack, PropertyValue, ShapeKind, property_id,
};
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

    let mut plan = PropertyPlan::for_actor_kind(ActorKindId::Shape(ShapeKind::Rect));
    let position = property_id("position").expect("position is registered");
    if let Some(slot) = plan.get_mut(position) {
        slot.track.add_keyframe(0, PropertyValue::Vec2([0.0, 0.0]));
        slot.track.add_keyframe(1000, PropertyValue::Vec2([100.0, 50.0]));
    }

    c.bench_function("property_plan_lookup_and_sample", |b| {
        b.iter(|| {
            let slot = plan.get(position).expect("position slot");
            black_box(slot.track.sample(black_box(500)));
        })
    });
}

criterion_group!(benches, bench_property_interpolation);
criterion_main!(benches);
