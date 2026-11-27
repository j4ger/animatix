# Animatix Studio

Animatix Studio (`animatix-gui`) combines source editing, preview, and timing review. The `.amx` source
and its referenced files define scene playback and export. The companion
`.amxn` redb database holds the workbench's recording history.

## Workflow

1. Open an `.amx` file from the filename menu. **Write** gives the editor most of
   the upper workbench and keeps the preview alongside it. **Review** gives the
   preview and timeline more room. In narrow windows, use the layout menu to
   switch modes. **Project** shows workspace folders, files, and resources. Opening a
   scene expands its parent folders in the file tree. Expanded folders load in the
   background. Use **Reload** in the filename menu to refresh the directory cache.
   **Outline** shows scenes and objects. Both open from the icon rail when space is tight.
2. Use the controls below the preview to play, seek, step frames, or loop. The
   timeline fits its visible lanes when their content changes. Drag the divider to set a custom height.
   Drag the ruler to scrub freely or hold Shift to land on a nearby keyframe. The
   timeline bracket button sets the loop to the visible interval; its range
   row appears when a loop range is set. The playback menu also offers speed,
   ping-pong, and presentation mode. Grid, guides, and preview zoom are above
   the scene. Use the preview's fullscreen button and press Escape to return.
   These actions affect the workspace. Scene and object lists change the selection.
3. Open **Inspector** to see the selected object and its sampled values. Pin it
   when you want to keep it visible. In a narrow window, a pinned Inspector moves
   to an overlay and returns to the dock when the window widens. **Go to source**
   opens the shared editor when a direct, unambiguous location is available.
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

Open **Voiceover…** beside a paragraph in Write, from the file menu, or from
the selected paragraph's controls in Review. **Record**
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

In Review, select a paragraph in the timeline or use the selector above the
paragraph track to see its words. **Voiceover…** opens the existing take and
alignment controls for that paragraph. Use **Bindings** to inspect its timing
targets and **Go to script** to return to the text. The selector includes paragraphs awaiting
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
Edit the source to change the animation. For standalone curve comparison,
use **Toggle Curve Comparison** in the command palette to open or close the
optional **Curves** tab.

## Read-only boundaries

Inspector fields, canvas transforms, object insertion, scene reordering and
keyframe property editing are not writing channels. Their old commands, controls,
handlers and insertion palette have been retired. Source editor changes and their
undo/redo remain available. Change the scene colorscheme in the source config.
The application theme is a separate preference.

## Settings

Open Settings from the workbench menu or command palette. It is an in-app page,
with section navigation on the left at wider widths and a selector above the
form at narrower widths. Playback pauses on entry. Back to workbench returns to
the same document, workspace layout, selection and playhead. **Interface
language** changes the workbench between English and Simplified Chinese
immediately. The choice is saved for the next launch. The command palette
accepts English search terms in either language. Scene source, identifiers and
timecodes retain their authored forms.

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
tempo. Authored markers occupy the lower band of the time ruler. Dense or unresolved
markers can be selected from the marker menu. The menu also lists compiled
`#cue:name` anchors by playback time. Select a cue to seek and, when its source
belongs to the current document, jump to its declaration. In a composition,
the scene label disambiguates repeated names. Named cues do not add extra stops
to the previous/next keyframe controls. Debug property tracks
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
Project and Outline switch together to named, tooltipped icons as space
tightens. Inspector summaries align compact
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
