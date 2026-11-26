---
name: animatix-explainer
description: Turn a topic or source into a complete page-first or narrated Animatix explainer, from main text through storyboard and scene handoff. Use for full explanation planning, not standalone AMX implementation.
metadata:
  version: "0.8.5-review-candidate"
---

# Animatix explainer

Use two separate human approvals by default: **main text**, then **storyboard**. For an internal prototype only, explicit delegation can put the coordinator at each gate; record the exact revision and decision. Silence and reviewer feedback are not approval.

1. **Write the text.** Draft the page in reading order or narration in speaking order, with stable section/cue IDs. Give a first-time viewer a concrete reason to stay before definitions. Read narration aloud; lead with concrete events rather than rhetorical pushback or instructions to watch. Preserve the mechanism, flag uncertainty, and stop for approval of that exact revision.
2. **Split the viewing task, not the shot.** Map approved text to [scene cards](assets/scene-card.md). Each beat needs a visible cause and a viewer inference. Show prerequisites before use; carry unchanged objects across beats; cut for a real reset. Leave room to read, and distinguish event time from motion that explains its consequence. For shared state, reasoning, or timing, use [storyboard and internal review](references/internal-review.md). Stop for separate storyboard approval **before writing or delegating production AMX**.
3. **Produce and verify.** Hand off the approved cards, neighboring context, and delivery constraints to animatix-scene. Preserve the cards and name blockers instead of substituting a different story. Check the assembled page or playback: real interaction, reading time, cue alignment, and whether the words and picture agree. For narrated work, align captions to the actual recording and inspect the cut, state, and caption together. Cue alignment alone does not prove word alignment. Report untested behavior.

Reopen the relevant gate if approved claims or beats change; recheck dependent scenes. For a fragile inference, run a [first-look check](references/internal-review.md#first-look-check) on the rendered scene with a fresh viewer. Give them the viewing material and one neutral question. Keep the script, cards and expected answer with the author.
