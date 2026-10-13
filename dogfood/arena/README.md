# Animatix Arena (Dogfood 2.0)

Animatix Arena is the comparative evaluation and feedback system for Animatix content,
new primitives, effects, DSL features, and generative agents. It brings an LLM-Arena-style
pairwise review model directly into the browser using the WebGPU `<amx-player>`.

## Concepts

| Term | Meaning |
|---|---|
| **Battle** | A staged matchup in `battles/<slug>/` comparing 2+ `.amx` candidate variants targeting the same brief. |
| **Candidate Variant** | An `.amx` scene expressing an alternative approach (e.g. declarative vs imperative, kinetic vs subtle, differing shader configurations). |
| **Arena Web App** | Browser UI (`web/arena/`) that runs synchronized WebGPU playback, frame-by-frame scrutiny, live `applySource` scratchpads, and side-by-side rubric voting. |
| **Feedback Log** | Structured, machine-readable JSON records (`feedback.json` per battle, aggregated in `feedback.jsonl`) that agents ingest to improve prompts, stdlib components, and compiler ergonomics. |

## Directory Layout

```text
dogfood/arena/
├── README.md               # This operating guide
├── feedback.jsonl          # Append-only history of all completed arena reviews
└── battles/                # Staged and completed arena battles
    └── <slug>/
        ├── battle.json     # Metadata: title, brief, tags, variants
        ├── brief.md        # Detailed goal description
        ├── a.amx           # Variant A
        ├── b.amx           # Variant B
        └── feedback.json   # Recorded decision, ratings, and timeline notes
```

## Review Workflow

### 1. Staging a Battle

Create candidates that satisfy the same content brief. Every candidate must pass `animatix check`:

```bash
python3 scripts/arena.py stage <slug> \
  --title "Glass Refraction vs Flat Border" \
  --brief "Animate a pricing card with depth" \
  --variants a.amx b.amx
```

### 2. Launching the Web Arena

Run the local server and open the Arena interface:

```bash
python3 scripts/arena.py serve
# Or open http://127.0.0.1:8124/arena/?battle=<slug>
```

### 3. Reviewing in the Browser

- **Synchronized Playback**: Scrub the master timeline to inspect motion timing, easing curves, and keyframes across all candidates simultaneously.
- **Speed Presets**: Use `0.25x` and `0.5x` slow-motion to verify subtle settle bounces, particle drift, and blur transitions.
- **Blind Battle Mode**: Toggle candidate titles and sources to judge pure visual polish and motion dynamics without author bias.
- **Inspection & Live Tweak**: Open the inline editor on any variant, edit parameters, and recompile live via `applySource` (`Ctrl+Enter`).
- **Submit Decision**: Rate on a 4-axis rubric (Visual Polish, Motion Dynamics, DSL Ergonomics, Brief Fidelity), add timeline-anchored notes, and click **Submit Decision**.

### 4. Ingesting Feedback (Agent Loop)

Agents read the structured verdict directly:

```bash
python3 scripts/arena.py ingest <slug>
```

The output gives:
- The winning variant.
- Timeline critiques (e.g. at $t=1.4\text{s}$, `"Border stroke too thick"`).
- Actionable directives for the next iteration or stdlib component promotion.
