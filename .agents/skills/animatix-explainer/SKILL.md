---
name: animatix-explainer
description: Turn a topic or source into a complete page-first or narrated Animatix explainer, from main text through storyboard and scene handoff. Use for full explanation planning, not standalone AMX implementation.
metadata:
  version: "0.8.9-review-candidate"
---

# Animatix explainer

Use two separate human approvals by default: **main text**, then **storyboard**. For an internal prototype only, explicit delegation can put the coordinator at each gate; record the exact revision and decision. Silence and reviewer feedback are not approval.

1. **Write to the viewer's question.** Before drafting, use the brief to decide privately what this viewer wants to understand or do and how far the answer needs to go. For a title alone, choose one question. If different meanings give different answers, name the precise concept in the definition and in any standalone caption or formula. A viewer who sees just that line should know what it describes. Start with a situation they recognize or a problem they already want to solve. Draft the page in reading order or narration in speaking order, with stable section/cue IDs.

   Check each step against that question. Supply the link the viewer needs to get from one step to the next. Give each needed term or symbol a clear referent and keep its name consistent. Preserve requested derivations and conditions or uncertainty that affect the answer. Cut lines that announce the lesson, repeat an answer the viewer already has, or open a different question.

   For narration, read only the spoken lines aloud, skipping display text and directions. Listen for where someone would have to guess what changed or how you got the result. Rewrite that line as you would say it to one person. If an equation carries a step, write the words that explain the operation and its result in the voice cue. Put the written equation beside it as a display cue. Stop for approval of the exact main text, whether page or narration.

2. **Split the viewing task, not the shot.** Map approved text to [scene cards](assets/scene-card.md). At each beat, identify what the viewer can now see or infer. Where the text supplies a link the picture alone leaves open, pair that line with the visible evidence for it. Show cause and effect for a mechanism, or align the features being compared for a comparison. Show prerequisites before use; carry unchanged objects across beats; cut for a real reset. Leave room to read, and distinguish event time from motion that explains its consequence. For shared state, reasoning, or timing, use [storyboard and internal review](references/internal-review.md). Stop for separate storyboard approval **before writing or delegating production AMX**.
3. **Produce and verify.** Hand off the approved cards, neighboring context, and delivery constraints to [animatix-scene](../animatix-scene/SKILL.md). When several cards need the same new object, identify one owning scene and the other callers so production can share one module. Pass along the project’s style frame or art direction when available. Preserve the cards and name blockers instead of substituting a different story. Check the assembled page or playback: real interaction, reading time, cue alignment, and whether the words and picture agree. If the viewer follows the same subject across a cut, compare the preceding held frame with the next readable frame at delivery size. Can you recognize the subject and tell what changed? For narrated work, align captions to the actual recording and inspect the cut, state, and caption together. Cue alignment alone does not prove word alignment. Report untested behavior.

Reopen the relevant gate if approved claims or beats change; recheck dependent scenes. For a fragile inference, run a [first-look check](references/internal-review.md#first-look-check) on the rendered scene with a fresh viewer. Give them the viewing material and one neutral question. Keep the script, cards and expected answer with the author.
