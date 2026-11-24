# Code-first GUI workbench

The GUI combines source editing, preview, and timing review. The `.amx` source
and its referenced files define scene playback and export. The companion
`.amxn` database holds the workbench's recording history.

## Workflow

1. Open an `.amx` file from the filename menu. Choose **Write** for source editing,
   **Review** for timing, or **Inspect** for object analysis. **Focus** hides the
   surrounding panels. Smaller windows group the presets in the layout menu.
2. Use the controls below the preview to play, seek, step frames, or loop. The
   timeline bracket button sets the loop to the visible interval; its range
   row appears when a loop range is set. The playback menu also offers speed,
   ping-pong, and presentation mode. Grid, guides, and preview zoom are above
   the scene. These actions affect the workspace. Scene and object lists change
   the selection.
3. Inspect an object and its sampled values, or jump to its source when a direct,
   unambiguous source location is available. Generated/imported instances do not
   guess a location by label.
4. Edit code, then review the rebuilt result. The footer distinguishes current,
   rebuilding, failed, and out-of-date previews. A failed build preserves the
   last successful image. Click the diagnostics button to see build messages.

The editor uses one source-text buffer. Keyframe headers, comments and blank
lines are directly editable alongside the rest of the file. The old notebook
cells, structural cell actions and semantic AST-edit API have been retired.
Syntax highlighting, completion, find/replace, explicit formatting, diagnostics
and source navigation remain available.

## Writing and reviewing narrated scenes

Open `crates/animatix/tests/fixtures/amx/animation/41_speech_sync_captions.amx` to try a scene with a recorded voice. In Write, **Only script** shows editable standalone `///` paragraphs. Use **Add paragraph** to start a draft. Consecutive narration lines form one paragraph, and a blank line starts another. **Only code** hides the script and supports localized code edits. Switch to **Full source** for changes that span a hidden paragraph or use formatting, completion, and source diagnostics.

Select a word or phrase in **Only script** and click **Bind phrase**, then choose **Start** or **End** and a timing target in the scene. **Bind paragraph** uses the full paragraph boundary. Timing targets include authored keyframes and direct actions that can receive independent timing while preserving their siblings. Each binding updates the source in one undoable edit. **Bound cues** links back to the source and to timing in Review.

When you edit one paragraph, the editor carries whole-paragraph bindings and
unambiguous phrase bindings into the new text. If the phrase disappeared or
has multiple possible matches, use **Rebind timing…** by the affected cue, or
open the **cues need timing…** menu when several cues need attention. The editor
preserves the start or end choice and the timing offset. During the editing
session, draft builds can use the last resolved cue time.

Open **Voiceover…** beside the paragraph or from the file menu. **Record**
uses the default microphone. After **Stop**, the workbench keeps the audio,
aligns it with local SenseVoice, and applies the result. The recording limit
is ten minutes or 128 MiB of PCM audio, whichever comes first. **More** contains
audio import, voice generation, and speech alignment settings.

Expand **History** to audition recordings and see the script each was recorded
against. **Use this take** is available for an aligned take that matches the
current text. Applying a take updates the audio and transcript references in
one source edit. Use the editor's normal Undo, Redo, and Save commands. If the
target script or current narration changes during processing, the new take
stays in history. [Set up and share a narrated scene](narration.md#record-in-the-native-workbench)

In Review, click a paragraph in the timeline or choose it from the paragraph
selector to see its words. Use **Bindings** to inspect the actions and
**Go to script** to return to the text. The selector includes paragraphs awaiting
alignment, and the selected paragraph still shows any voiceover issue. External
changes to an attached audio file or transcript refresh the preview, including
when the script has unsaved edits.

## Inspecting an animation curve

In Review, enable **Property tracks** with the sliders button in the Timeline
toolbar. Expand an animated object's properties using the list icon beside its
name. Click the graph icon on a keyed property row to show the read-only curve
directly underneath that row; click again to close it. Vector and color tracks
offer a channel selector beside the graph. The graph shares the Timeline time
scale and playhead, and clicking or dragging it seeks the preview. It shows
compiled keyframe interpolation, not subsequent expression or layout effects.
Edit the source to change the animation. The standalone Curves panel remains
available for viewing multiple objects together.

## Read-only boundaries

Inspector fields, canvas transforms, object insertion, scene reordering and
keyframe property editing are not writing channels. Their old commands, controls,
handlers and insertion palette have been retired. Source editor changes and their
undo/redo remain available. Settings displays the scene colorscheme for inspection;
change it in the source config. IDE appearance controls affect the application theme.

The timeline presents author timing rather than an always-expanded forest of ECS
properties. Unresolved source timing must be shown as unresolved, not silently
invented. A source navigation target is only offered for the current source.

Audio waveforms are review aids based on cached min/max envelopes. Transcript and
waveform displays do not move media or change export values. Output-device absence
is not proof of audible playback; device/synchronization acceptance remains a
separate platform check.

## Deliberate limits

The workbench remains source-first, with read-only inspection and timing review.
It does not implement arbitrary visual editing or drag-to-rewrite timing. Any
future authoring surface must use a narrowly scoped, visible and undoable source
patch, with source revision checks. Whole-file AST reserialization is not a safe substitute.

An integrated audit/storyboard evidence browser and complete component/loop source
provenance are still separate work. Raw source, derived build state and workspace
view state must remain distinct as those features are added.

### Review data semantics

The first phase indexes direct scene/top-level `#` declarations. Numeric and
relative seconds and speech anchors can be resolved; beat-based or generated
control-flow timing is explicitly unresolved rather than inferred with a guessed
tempo. Dense markers can be selected from the marker menu. Debug property tracks
are opt-in, read-only, and remain distinct from authored keyframes.

Audio-backed transcript cues use the clip's start in the scene and the scene's start in the composition. A standalone transcript uses scene-relative timestamps. Source timing markers use the engine's compiled resolution for speech. Identical Audio/Captions transcripts at the same time share a review row.

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
Project, Outline and Library keep their names as space tightens, before
switching together to named, tooltipped icons. Inspector summaries align compact
labels and values in two columns, stacking them when space is tight. Preview
ruler labels occupy a separate lane from the ticks; drag from either ruler to
place a guide.

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


For interface implementation guidance, see [GUI design language](gui_design_language.md).
