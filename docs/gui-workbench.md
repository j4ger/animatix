# Code-first GUI workbench

The GUI is a source editor with a preview and a time-review workbench, not a second
scene authoring model. The `.amx` source, imports, audio and transcripts are the
source of truth; no timing sidecar is introduced.

## Workflow

1. Open an `.amx` file from the filename menu. Choose **Write** for source editing,
   **Review** for timing, or **Inspect** for object analysis. **Focus** hides the
   surrounding panels. Smaller windows group the presets in the layout menu.
2. Use the controls below the preview to play, seek, step frames, or loop. The
   playback menu also offers speed, ping-pong, and presentation mode. Grid,
   guides, and preview zoom are above the scene. These actions affect the
   workspace. Scene and object lists change the selection.
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

Open `examples/animation/41_speech_sync_captions.amx` to try a scene with a recorded voice. In Write, **Only script** shows editable standalone `///` paragraphs. Use **Add paragraph** to start a draft. Consecutive narration lines form one paragraph, and a blank line starts another. **Only code** hides the script and supports localized code edits. Switch to **Full source** for changes that span a hidden paragraph or use formatting, completion, and source diagnostics.

Select a word or phrase in **Only script** and click **Bind phrase**, then choose **Start** or **End** and a timing target in the scene. Choose **Bind paragraph** to use the full paragraph boundary. Timing targets include authored keyframes and direct actions that can receive independent timing while preserving their siblings. Each binding updates the source in one undoable edit. Use the command palette's **Undo** or **Redo** to step through those edits, and save when ready. Selecting the second occurrence binds that occurrence even if a recognizer splits the surrounding words differently. Word times come from the selected recording. Editing the paragraph can require selecting the phrase again. **Bound cues** links back to the source and to timing in Review.

If a script revision leaves a cue without a matching source paragraph, use **Repair binding…** in **Only script**. The dialog highlights possible occurrences in context. Choose the intended occurrence or use **Select in script** to pick replacement text yourself. Repair keeps the original start or end choice and any authored timing offset.

Choose **Voice takes…** beside the paragraph, or **Narration Takes…** from the file menu. Under **Record or import audio**, choose **Record** to use the default microphone and **Stop** when the reading is complete. **Discard** clears the temporary WAV. Each take fits up to ten minutes or 128 MiB of PCM audio, whichever comes first. You can also import an existing audio file.

For either kind of recording, supply its word timing JSON or align it with a local SenseVoice model. To generate speech from the paragraph, choose Edge or a configured local provider such as Qwen. Audition the candidate and adopt the reading you want. Adoption updates the document’s media references in one undoable edit and keeps previous versions available. If the source changes while a background job runs, the editor asks you to retry against the current revision.

In Review, click a paragraph in the timeline or select it from the Script menu to see its words. Use **Bindings** to inspect the actions and **Show in Write** to return to the text. A paragraph awaiting alignment remains accessible in the menu. External changes to an attached audio file or transcript refresh the preview, including when the script has unsaved edits.

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
