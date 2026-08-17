#!/usr/bin/env bash
# Generate a static HTML questionnaire/arena for a dogfood review run.
#
# Usage:
#   bash scripts/review-report.sh <slug> [output.html]
#
# The output page is self-contained for sources and comments; frame images are
# linked from the run directory when present (e.g. frames/*.png). Reviewer
# choices and new notes are kept in localStorage and can be exported as JSON,
# which keeps this useful as a questionnaire without a server.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

SLUG="${1:-}"
if [[ -z "$SLUG" ]]; then
  echo "usage: bash scripts/review-report.sh <slug> [output.html]" >&2
  exit 2
fi

RUN_DIR="$PROJECT_DIR/dogfood/runs/$SLUG"
if [[ ! -d "$RUN_DIR" ]]; then
  echo "review run not found: $RUN_DIR" >&2
  exit 2
fi

OUTPUT="${2:-$RUN_DIR/review-report.html}"

AMX_COUNT="$(find "$RUN_DIR" -maxdepth 1 -type f \( -name '*.amx' -o -name '*.proposed' -o -name '*.amx.proposed' \) | wc -l | tr -d ' ')"
if [[ "$AMX_COUNT" -lt 2 ]]; then
  echo "review run needs at least two variants (.amx or .proposed): $RUN_DIR" >&2
  exit 2
fi

COMMENTS_JSON="$RUN_DIR/review.json"
if [[ -f "$COMMENTS_JSON" ]]; then
  COMMENTS="$(cat "$COMMENTS_JSON")"
else
  COMMENTS='{"comments":[]}'
fi

BRAND="Animatix Review"
TITLE="$BRAND: $SLUG"
OUT_ABS="$(cd "$(dirname "$OUTPUT")" && pwd)/$(basename "$OUTPUT")"

{
cat <<HTML
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${TITLE}</title>
<style>
  :root { color-scheme: light dark; --bg:#101418; --panel:#171d24; --line:#2a333d; --text:#e8edf2; --muted:#9aa7b2; --accent:#4ea1ff; --good:#6fcf97; --bad:#eb5757; }
  * { box-sizing:border-box; }
  body { margin:0; background:var(--bg); color:var(--text); font:15px/1.5 system-ui, sans-serif; }
  header { padding:24px 32px; border-bottom:1px solid var(--line); }
  h1 { margin:0; font-size:22px; }
  .meta { color:var(--muted); margin-top:6px; }
  main { display:grid; grid-template-columns:1fr 1fr; gap:20px; padding:24px 32px; }
  @media (max-width: 900px) { main { grid-template-columns:1fr; } }
  section { background:var(--panel); border:1px solid var(--line); border-radius:8px; padding:18px; }
  h2 { margin:0 0 10px; font-size:16px; }
  .variant-id { color:var(--accent); font-weight:600; }
  pre { white-space:pre-wrap; overflow-wrap:anywhere; max-height:520px; overflow:auto; background:#0c1014; border:1px solid var(--line); padding:12px; border-radius:6px; font-size:12px; }
  .frames { display:flex; flex-wrap:wrap; gap:8px; margin:10px 0; }
  .frames img { max-width:260px; border-radius:6px; border:1px solid var(--line); }
  .comment { border-left:3px solid var(--accent); padding:8px 12px; margin:8px 0; background:#111821; }
  .controls { display:flex; gap:10px; margin-top:12px; flex-wrap:wrap; }
  button, textarea { font:inherit; background:#1e2730; color:var(--text); border:1px solid var(--line); border-radius:6px; padding:8px 12px; }
  button:hover { border-color:var(--accent); }
  textarea { width:100%; min-height:90px; resize:vertical; }
  .choice { display:inline-flex; gap:8px; align-items:center; }
  .choice button.on { border-color:var(--accent); color:var(--accent); }
  .footer { padding:12px 32px 32px; color:var(--muted); }
</style>
</head>
<body>
<header>
  <h1>${TITLE}</h1>
  <div class="meta">Run directory: <code>${RUN_DIR}</code></div>
  <div class="meta">Pick the clearer expression for the same brief, add notes, then export JSON for the agent.</div>
</header>
<main>
HTML

while IFS= read -r amx; do
  NAME="$(basename "$amx")"
  STEM="${NAME%.amx}"
  STEM="${STEM%.proposed}"
  echo '<section>'
  echo "<h2>Variant <span class=\"variant-id\">${STEM^^}</span></h2>"
  echo "<div class=\"frames\">"
  for frame in "$RUN_DIR"/frames/*"${STEM}"*.png "$RUN_DIR"/frames/*"${STEM}"*.webp; do
    if [[ -f "$frame" ]]; then
      REL="${frame#"$RUN_DIR"/}"
      echo "<img src=\"${REL}\" alt=\"${STEM} frame\">"
    fi
  done
  echo '</div>'
  echo '<pre>'
  sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' "$amx"
  echo '</pre>'
  echo '<button data-variant="'${STEM}'" class="choose-a">Prefer A</button>'
  echo '<button data-variant="'${STEM}'" class="choose-b">Prefer B</button>'
  echo '<label><input type="checkbox" data-variant="'${STEM}'" class="blocker"> Blocker issue</label>'
  echo '</section>'
done < <(find "$RUN_DIR" -maxdepth 1 -type f \( -name '*.amx' -o -name '*.proposed' -o -name '*.amx.proposed' \) | sort)

cat <<HTML
</main>
<section class="footer" style="margin:0 32px 24px;background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:18px;">
  <h2 style="margin-top:0">Comments</h2>
  <div id="comments"></div>
  <textarea id="note" placeholder="New note (optional time anchor: e.g. 1.25s)"></textarea>
  <div class="controls">
    <button id="add">Add note</button>
    <button id="export">Export JSON</button>
    <button id="clear">Clear local answers</button>
  </div>
</section>
<div class="footer">Generated by <code>scripts/review-report.sh</code>.</div>
<script>
const RUN = ${COMMENTS};
const KEY = 'animatix-review:' + '${SLUG}';
const answers = JSON.parse(localStorage.getItem(KEY) || '{}');
const notes = JSON.parse(localStorage.getItem(KEY + ':notes') || '[]');

function renderComments() {
  const el = document.getElementById('comments');
  const all = RUN.comments || [];
  const rows = all.map(c =>
    '<div class="comment"><b>' + c.variant + '</b>' +
    (c.time_ms != null ? ' @ ' + (c.time_ms / 1000).toFixed(2) + 's' : '') +
    ' [' + c.severity + '] ' + c.note.replace(/[<>&]/g, s => ({'<':'&lt;','>':'&gt;','&':'&amp;'}[s])) +
    '</div>').join('');
  el.innerHTML = rows || '<div class="meta">No committed comments yet.</div>';
}

function syncButtons() {
  document.querySelectorAll('.choose-a').forEach(b => b.classList.toggle('on', answers[b.dataset.variant] === 'a'));
  document.querySelectorAll('.choose-b').forEach(b => b.classList.toggle('on', answers[b.dataset.variant] === 'b'));
  document.querySelectorAll('.blocker').forEach(b => b.checked = answers['blocker:' + b.dataset.variant] === true);
}

document.querySelectorAll('.choose-a').forEach(b => b.addEventListener('click', () => {
  answers[b.dataset.variant] = 'a'; localStorage.setItem(KEY, JSON.stringify(answers)); syncButtons();
}));
document.querySelectorAll('.choose-b').forEach(b => b.addEventListener('click', () => {
  answers[b.dataset.variant] = 'b'; localStorage.setItem(KEY, JSON.stringify(answers)); syncButtons();
}));
document.querySelectorAll('.blocker').forEach(b => b.addEventListener('change', () => {
  answers['blocker:' + b.dataset.variant] = b.checked; localStorage.setItem(KEY, JSON.stringify(answers));
}));
document.getElementById('add').addEventListener('click', () => {
  const raw = document.getElementById('note').value.trim();
  if (!raw) return;
  notes.push({ id: crypto.randomUUID(), source: 'external', severity: 'Question', note: raw });
  localStorage.setItem(KEY + ':notes', JSON.stringify(notes));
  document.getElementById('note').value = '';
  renderComments();
});
document.getElementById('export').addEventListener('click', () => {
  const payload = { run: '${SLUG}', answers, new_notes: notes, existing: RUN };
  const blob = new Blob([JSON.stringify(payload, null, 2)], {type: 'application/json'});
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a'); a.href = url; a.download = 'review-external.json'; a.click();
  URL.revokeObjectURL(url);
});
document.getElementById('clear').addEventListener('click', () => {
  localStorage.removeItem(KEY); localStorage.removeItem(KEY + ':notes'); location.reload();
});

renderComments(); syncButtons();
</script>
</body>
</html>
HTML
} > "$OUT_ABS"

echo "Wrote static review report: $OUT_ABS"
