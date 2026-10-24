use super::*;

#[test]
fn directly_inserted_tracks_get_default_meta_on_demand() {
    let mut world = World::new();
    world.tracks.insert("direct".into(), AnimationTrack::new("direct".into()));
    let id = world.tracks.get_id("direct").expect("direct id");

    assert!(world.meta(id).visible, "missing component reads as the default");
    world.meta_mut(id).visible = false;
    assert!(!world.meta(id).visible);
    assert_eq!(world.first_seen(id), u64::MAX);
    world.set_first_seen(id, 250);
    assert_eq!(world.first_seen(id), 250);
    world.set_first_seen(id, 900);
    assert_eq!(world.first_seen(id), 250, "first sighting wins");
}
