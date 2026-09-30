// Live figure editors for the language tour.
//
// Each tour figure marked `data-editable` gains an "edit this figure" toggle.
// Editing swaps the figure into a two-column view — animation beside a
// textarea — where Apply (or Ctrl/Cmd+Enter) rebuilds the scene in place
// through `amx-player.applySource` and surfaces the engine's own diagnostics
// (the same set the LSP reports). Reset restores the original source; Done
// closes the editor and leaves the scene as the editor left it. One editor
// open at a time; edits live only on the page.

(function () {
  const originals = new Map(); // figure -> source text at first open
  let active = null; // { figure, textarea, panel, status }

  function diagLine(d) {
    const at = d.line ? ` (line ${d.line}${d.column ? ":" + d.column : ""})` : "";
    const subject = d.subject ? ` · ${d.subject}` : "";
    return `${d.code}${at}${subject} — ${d.message}`;
  }

  function renderDiags(panel, diags) {
    panel.textContent = "";
    if (!diags.length) {
      const ok = document.createElement("p");
      ok.className = "diag-ok";
      ok.textContent = "No issues — the build is clean.";
      panel.append(ok);
      return;
    }
    for (const d of diags) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = `diag diag-${d.severity}`;
      row.textContent = diagLine(d);
      if (d.line) {
        // Click a diagnostic to move the caret to its line.
        row.addEventListener("click", () => {
          const ta = active?.textarea;
          if (!ta) return;
          const lines = ta.value.split("\n");
          let offset = 0;
          for (let i = 0; i < Math.min(d.line - 1, lines.length); i += 1) {
            offset += lines[i].length + 1;
          }
          ta.focus();
          ta.setSelectionRange(offset, offset + (lines[d.line - 1]?.length ?? 0));
        });
      }
      panel.append(row);
    }
  }

  function setStatus(status, text, tone) {
    status.textContent = text;
    status.className = `editor-status ${tone ?? ""}`;
  }

  function apply(figure, player, textarea, panel, status) {
    const text = textarea.value;
    try {
      const diags = player.applySource(text);
      const warnings = diags.filter((d) => d.severity !== "error");
      renderDiags(panel, diags);
      setStatus(
        status,
        warnings.length
          ? `Built — ${warnings.length} warning${warnings.length === 1 ? "" : "s"}.`
          : "Built — the scene is playing your version.",
        warnings.length ? "warn" : "ok",
      );
    } catch (err) {
      renderDiags(panel, err.diagnostics ?? []);
      setStatus(status, `Not built: ${err.message}`, "error");
    }
  }

  function close() {
    if (!active) return;
    const { figure } = active;
    figure.classList.remove("editing");
    figure.querySelector(".editor-col").hidden = true;
    figure.querySelector(".edit-toggle").textContent = "edit this figure";
    active = null;
  }

  async function open(figure) {
    close();
    const player = figure.querySelector("amx-player");
    const col = figure.querySelector(".editor-col");
    const textarea = col.querySelector("textarea");
    const panel = col.querySelector(".diag-panel");
    const status = col.querySelector(".editor-status");

    if (!originals.has(figure)) {
      try {
        const response = await fetch(player.getAttribute("src"));
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        originals.set(figure, await response.text());
      } catch (err) {
        setStatus(status, `Could not fetch the scene source: ${err.message}`, "error");
        return;
      }
    }
    textarea.value = originals.get(figure);
    renderDiags(panel, []);
    setStatus(status, "Edit, then Apply (Ctrl/Cmd+Enter) to rebuild the scene.");
    figure.classList.add("editing");
    col.hidden = false;
    figure.querySelector(".edit-toggle").textContent = "close the editor";
    active = { figure, textarea, panel, status };
    // Bring the figure on screen: offscreen embeds are lazy, so an editor
    // opened from the toc would otherwise Apply against a player that has
    // not started loading.
    figure.scrollIntoView({ behavior: "smooth", block: "center" });
    textarea.focus();
  }

  function buildEditorCol() {
    const col = document.createElement("div");
    col.className = "editor-col";
    col.hidden = true;

    const bar = document.createElement("div");
    bar.className = "editor-bar";
    const title = document.createElement("span");
    title.className = "editor-title";
    title.textContent = "scene.amx";
    const apply = document.createElement("button");
    apply.type = "button";
    apply.className = "editor-btn primary";
    apply.textContent = "Apply";
    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "editor-btn";
    reset.textContent = "Reset";
    const done = document.createElement("button");
    done.type = "button";
    done.className = "editor-btn";
    done.textContent = "Done";
    bar.append(title, done, reset, apply);

    const textarea = document.createElement("textarea");
    textarea.className = "editor-text";
    textarea.spellcheck = false;
    textarea.setAttribute("aria-label", "Scene source");

    const status = document.createElement("p");
    status.className = "editor-status";
    const panel = document.createElement("div");
    panel.className = "diag-panel";

    col.append(bar, textarea, status, panel);
    return { col, textarea, panel, status, apply, reset, done };
  }

  function init() {
    for (const figure of document.querySelectorAll("figure[data-editable]")) {
      const player = figure.querySelector("amx-player");
      if (!player) continue;

      const row = document.createElement("div");
      row.className = "stage-row";
      const stageCol = document.createElement("div");
      stageCol.className = "stage-col";
      player.replaceWith(row);
      stageCol.append(player);
      const { col, textarea, panel, status, apply: applyBtn, reset, done } = buildEditorCol();
      row.append(stageCol, col);

      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "edit-toggle";
      toggle.textContent = "edit this figure";

      toggle.addEventListener("click", () => {
        if (figure.classList.contains("editing")) close();
        else open(figure);
      });
      applyBtn.addEventListener("click", () => apply(figure, player, textarea, panel, status));
      reset.addEventListener("click", () => {
        textarea.value = originals.get(figure) ?? textarea.value;
        apply(figure, player, textarea, panel, status);
      });
      done.addEventListener("click", close);
      textarea.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
          event.preventDefault();
          apply(figure, player, textarea, panel, status);
        }
        if (event.key === "Escape") close();
      });

      const caption = figure.querySelector("figcaption");
      if (caption) caption.append(toggle);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
