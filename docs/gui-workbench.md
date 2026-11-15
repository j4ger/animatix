# Code-first GUI workbench

The GUI is a source editor with a preview and a time-review workbench, not a second
scene authoring model. The `.amx` source, imports, audio and transcripts are the
source of truth; no timing sidecar is introduced.

## Workflow

1. Open an `.amx` file. Use **Write** for source editing, **Review** for timing and
   **Inspect** for object analysis. **Focus** hides the surrounding panels.
2. Play, seek, step frames, or select a review loop. These are workspace actions,
   not document changes. Scene and object lists select rather than rearrange.
3. Inspect an object and its sampled values, or jump to its source when a direct,
   unambiguous source location is available. Generated/imported instances do not
   guess a location by label.
4. Edit code, then review the rebuilt result. The status strip distinguishes
   current, rebuilding, failed, and out-of-date previews. A successful preview is
   not an audit result. A failed build preserves the last successful image.

## Read-only boundaries

Inspector fields, canvas transforms, object insertion, scene reordering and
keyframe property editing are not writing channels. Commands are guarded centrally
as well as removed from their controls, so stale selections or keyboard shortcuts
cannot silently write source. Source editor changes and their undo/redo remain
available.

The timeline presents author timing rather than an always-expanded forest of ECS
properties. Unresolved source timing must be shown as unresolved, not silently
invented. A source navigation target is only offered for the current source.

Audio waveforms are review aids based on cached min/max envelopes. Transcript and
waveform displays do not move media or change export values. Output-device absence
is not proof of audible playback; device/synchronization acceptance remains a
separate platform check.

## Deliberate limits

This change is the first, read-only phase of the workbench design. It does not
implement arbitrary visual editing or drag-to-rewrite timing. A future timing
editor must use a narrowly scoped, visible and undoable source patch, with source
revision checks. Existing AST reserialization is not a safe substitute.

An integrated audit/storyboard evidence browser and complete component/loop source
provenance are still separate work. Raw source, derived build state and workspace
view state must remain distinct as those features are added.

### Review data semantics

The first phase indexes direct scene/top-level `#` declarations. Numeric and
relative seconds and speech anchors can be resolved; beat-based or generated
control-flow timing is explicitly unresolved rather than inferred with a guessed
tempo. Dense markers can be selected from the marker menu. Debug property tracks
are opt-in, read-only, and remain distinct from authored keyframes.

Audio is placed using each segment's start, including its delay and composition
scene offset. Transcript cues use the engine's existing speech time semantics:
transcript timestamps plus scene offset, **not** an inferred Audio delay. A delayed
audio track with an unshifted transcript can therefore remain misaligned; fixing
that requires an explicit engine/transcript contract rather than silently moving
GUI markers. Identical Audio/Captions transcripts share a review row.

Inspector values are sampled tracks/defaults, not a full post-layout ECS dump.
Expression-driven and layout-derived final values can differ. Direct source links
are withheld for stale builds and ambiguous/generated/imported declarations.

### Compact controls

Workbench actions use the existing Phosphor icon family, with descriptive tooltips
and readable accessibility names rather than glyph-only names. Playback, source
marker navigation, review loops and panel controls remain separate groups. Selected
view modes retain a visible active state; unavailable actions remain disabled.

Filename, preview validity, timecode, scene names, track names and panel headings
remain text because they describe the current document rather than an action.
Action descriptions and keyboard shortcuts belong in tooltips. Iconification does
not expand editing permissions: timeline controls still change review state only.

### Validation and remaining platform checks

The November workbench pass recorded native Linux checks at approximately 1180 px
and 836 px window widths: tooltip discovery, source-marker navigation, menu
cancellation and review-loop controls. GUI unit tests, workspace checks, strict
Clippy, formatting and structure gates passed for that change. This is historical
coverage, not a guarantee that a later revision passes those checks.

Remaining acceptance work:

- The test machine had no usable audio output device. Waveform decoding and
  visual seeking were checked, but audible playback and synchronization were not.
- Windows and macOS runtime acceptance remains outstanding.
- Icon tests cover readable click-event names and disabled actions, not end-to-end
  screen-reader certification.
- A relative document path could leave the speech demo's audio asset unavailable;
  opening the same document by absolute path resolved the waveform. Use an
  absolute path until media-path resolution has a regression test and fix.

For interface implementation guidance, see [GUI design language](gui_design_language.md).
