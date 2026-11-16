# Animatix's Vello 0.11.0 changes

The workspace uses this local copy for two independent boundaries. Dependency
upgrades must preserve both until upstream provides equivalent behavior.

## Persistent image atlas

`src/render.rs` keeps an existing image atlas alive across image-free renders.
Discarding it while the resolver retains image residency makes a later image
render sample an empty atlas (upstream #1948 / #1558). Remove this repair when
the selected release contains the fix tracked by upstream #1664.

The native [image probe](../../animatix-render/tests/vello_img_probe.rs)
checks uploaded-image, image-free, and later-image sequences on a reused
renderer.

## Submit-scoped encoder suffix

`Renderer::render_to_texture_with_callback` appends an infallible, synchronous
callback to the current recording's encoder, after Vello commands and before
its queue submission. The ordinary API delegates with a no-op callback.
`WgpuEngine::run_recording_with_callback` retains the original order:

1. Encode the complete Vello recording.
2. Resolve any Vello profiler queries.
3. Encode the caller's suffix.
4. Submit the combined command buffer.
5. Recycle/free Vello's transient resources.

Recording errors return before the suffix is called. The API does not return
an encoder, hold work for another frame, or change resource-pool ownership.
Vello profiler measurements continue to exclude the suffix. A suffix must
only encode the following work; it cannot initiate another render through
the renderer currently borrowed by the call.

Animatix uses this boundary to sample the completed offscreen image in the
presentation blit without a second explicit queue submission. Backdrops,
filter composites, and transition composition retain their existing ordering;
they are not implicitly included by this API.

The native [presentation probe](../../animatix-render/tests/presentation_submit.rs)
compares merged and separate paths, including reused renderer/image resources
and independent targets. The engine's private error-boundary test checks that
a malformed recording does not invoke its suffix.

Remove this second change when the selected upstream release supports an
equivalent synchronous render-and-tail submission contract. The whole vendored
directory and Cargo patch can go only when both boundaries are covered.
