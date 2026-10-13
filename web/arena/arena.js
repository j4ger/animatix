// Animatix Arena — Precision Evaluation & Review Studio
// Synchronized WebGPU multi-player playback, rubric scoring, and agent feedback engine.

(function () {
  let currentBattle = null;
  let allBattles = [];
  let players = [];
  let activeMode = "blind"; // "blind" | "inspector"
  let isPlaying = false;
  let maxDuration = 3.0;
  let currentTime = 0;
  let playbackSpeed = 1.0;
  let rafId = null;

  // Review State
  let selectedDecision = null;
  let rubricScores = { visual: 4, motion: 4, ergonomics: 4, fidelity: 4 };
  let timelineNotes = [];

  // DOM Elements
  const battleSelect = document.getElementById("battle-select");
  const battleTitle = document.getElementById("battle-title");
  const briefText = document.getElementById("brief-text");
  const briefTags = document.getElementById("brief-tags");
  const arenaStage = document.getElementById("arena-stage");
  const playBtn = document.getElementById("master-play-btn");
  const timelineSlider = document.getElementById("master-slider");
  const timecodeDisplay = document.getElementById("timecode-display");
  const noteTimeInput = document.getElementById("note-time-input");
  const submitStatus = document.getElementById("submit-status");

  // Format seconds to string #0.00s
  function fmtTime(s) {
    return `#${Math.max(0, s).toFixed(2)}s`;
  }

  // Load battle list from API or built-in list
  async function loadBattleList() {
    try {
      const res = await fetch("/api/arena/battles");
      if (res.ok) {
        allBattles = await res.json();
      } else {
        throw new Error("API not available");
      }
    } catch (_) {
      // Fallback list for static hosting or direct file viewing
      allBattles = [
        { id: "001-stagger-entry", title: "Stagger Action vs Explicit Delays" },
        { id: "002-array-actors-vs-named", title: "Array Actors vs Hand-Named Actors" },
        { id: "003-fade-in-array-target", title: "Fade-in Array Target vs Container Target" }
      ];
    }

    battleSelect.innerHTML = "";
    allBattles.forEach((b) => {
      const opt = document.createElement("option");
      opt.value = b.id;
      opt.textContent = `${b.id}: ${b.title}`;
      battleSelect.appendChild(opt);
    });

    const urlParams = new URLSearchParams(window.location.search);
    const queryBattle = urlParams.get("battle") || urlParams.get("session");
    if (queryBattle && allBattles.some((b) => b.id === queryBattle)) {
      battleSelect.value = queryBattle;
    }

    loadBattle(battleSelect.value);
  }

  // Load a single battle's metadata and variants
  async function loadBattle(battleId) {
    players = [];
    timelineNotes = [];
    selectedDecision = null;
    currentTime = 0;
    isPlaying = false;
    updatePlayButton();
    renderNotesList();
    resetDecisionButtons();

    try {
      const res = await fetch(`/api/arena/battle/${battleId}`);
      if (res.ok) {
        currentBattle = await res.json();
      } else {
        // Fallback relative load
        const rawMeta = await fetch(`../../dogfood/arena/battles/${battleId}/battle.json`);
        currentBattle = await rawMeta.json();
      }
    } catch (err) {
      console.warn("Could not load battle via API, using fallback structure:", err);
      currentBattle = {
        id: battleId,
        title: battleId,
        brief: "Compare candidate implementations for this brief.",
        tags: ["dogfood", "arena"],
        variants: [
          { id: "a", label: "Candidate A", file: "a.amx" },
          { id: "b", label: "Candidate B", file: "b.amx" }
        ]
      };
    }

    battleTitle.textContent = currentBattle.title || currentBattle.id;
    briefText.textContent = currentBattle.brief || "No description provided.";
    briefTags.innerHTML = (currentBattle.tags || [])
      .map((t) => `<span class="tag-pill">${t}</span>`)
      .join("");

    renderStage();
  }

  // Render the candidate cards into the stage
  async function renderStage() {
    arenaStage.innerHTML = "";
    maxDuration = 3.0;

    const variants = currentBattle.variants || [
      { id: "a", label: "Candidate A", file: "a.amx" },
      { id: "b", label: "Candidate B", file: "b.amx" }
    ];

    for (let i = 0; i < variants.length; i++) {
      const v = variants[i];
      const card = document.createElement("div");
      card.className = "variant-card";
      card.id = `variant-card-${v.id}`;

      const displayName = activeMode === "blind"
        ? `Candidate ${String.fromCharCode(65 + i)}`
        : (v.label || `Variant ${v.id.toUpperCase()}`);

      let srcPath = v.source_url;
      if (!srcPath) {
        srcPath = `/dogfood/arena/battles/${currentBattle.id}/${v.file || v.id + ".amx"}`;
      }

      card.innerHTML = `
        <div class="variant-card-header">
          <div class="variant-meta">
            <span class="variant-badge">${String.fromCharCode(65 + i)}</span>
            <span class="variant-label" id="variant-label-${v.id}">${displayName}</span>
          </div>
          <button class="choice-btn" style="min-width:auto;padding:4px 10px;font-size:12px;" data-vote="${v.id}">
            Pick This
          </button>
        </div>
        <div class="variant-player-container">
          <amx-player src="${srcPath}" controls aspect="16:9"></amx-player>
        </div>
        <div class="variant-code-section" id="code-section-${v.id}" style="${activeMode === 'blind' ? 'display:none;' : ''}">
          <div class="code-header">
            <span>${v.file || v.id + '.amx'}</span>
            <button class="apply-tweak-btn" data-variant="${v.id}">Apply Tweak (Ctrl+Enter)</button>
          </div>
          <textarea class="code-editor-area" id="editor-${v.id}" spellcheck="false" placeholder="Loading source..."></textarea>
          <div class="diag-strip clean" id="diag-${v.id}">Clean build</div>
        </div>
      `;

      arenaStage.appendChild(card);

      const playerEl = card.querySelector("amx-player");
      players.push({ id: v.id, el: playerEl, card: card });

      // Fetch code source for inspector/scratchpad
      fetch(srcPath)
        .then((r) => (r.ok ? r.text() : "// Failed to load source"))
        .then((code) => {
          const ta = document.getElementById(`editor-${v.id}`);
          if (ta) ta.value = code;
        })
        .catch(() => {});

      // Pick This button click
      card.querySelector(`[data-vote="${v.id}"]`).addEventListener("click", () => {
        selectDecision(`prefer_${v.id}`);
      });

      // Apply tweak button
      const applyBtn = card.querySelector(`.apply-tweak-btn[data-variant="${v.id}"]`);
      if (applyBtn) {
        applyBtn.addEventListener("click", () => applyTweak(v.id));
      }

      const ta = card.querySelector(`#editor-${v.id}`);
      if (ta) {
        ta.addEventListener("keydown", (e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
            e.preventDefault();
            applyTweak(v.id);
          }
        });
      }
    }

    // Wait slightly for player elements to upgrade and query durations
    setTimeout(syncMaxDuration, 800);
  }

  function syncMaxDuration() {
    let longest = 0.1;
    players.forEach((p) => {
      const dur = p.el._duration || p.el.duration || 3.0;
      if (dur > longest) longest = dur;
    });
    maxDuration = Math.max(longest, 1.0);
    timelineSlider.max = maxDuration.toFixed(2);
    updateTimeDisplay();
  }

  // Live in-browser tweak using applySource
  function applyTweak(variantId) {
    const p = players.find((x) => x.id === variantId);
    const ta = document.getElementById(`editor-${variantId}`);
    const diag = document.getElementById(`diag-${variantId}`);
    if (!p || !ta || !diag) return;

    try {
      const diags = p.el.applySource(ta.value);
      const warnings = (diags || []).filter((d) => d.severity !== "error");
      if (warnings.length > 0) {
        diag.className = "diag-strip warn";
        diag.textContent = `Built with ${warnings.length} warning(s): ${warnings[0].message}`;
      } else {
        diag.className = "diag-strip clean";
        diag.textContent = "Built cleanly — playing revised scene.";
      }
    } catch (err) {
      diag.className = "diag-strip error";
      diag.textContent = `Build error: ${err.message}`;
    }
  }

  // Master Synchronized Playback Loop
  function playAll() {
    isPlaying = true;
    updatePlayButton();
    players.forEach((p) => {
      if (typeof p.el.play === "function") p.el.play();
    });
    startTickLoop();
  }

  function pauseAll() {
    isPlaying = false;
    updatePlayButton();
    players.forEach((p) => {
      if (typeof p.el.pause === "function") p.el.pause();
    });
    stopTickLoop();
  }

  function togglePlay() {
    if (isPlaying) {
      pauseAll();
    } else {
      playAll();
    }
  }

  function seekAll(time) {
    currentTime = Math.max(0, Math.min(time, maxDuration));
    timelineSlider.value = currentTime;
    updateTimeDisplay();
    players.forEach((p) => {
      if (typeof p.el.seek === "function") {
        p.el.seek(currentTime);
      }
    });
  }

  function stepFrame(forward) {
    pauseAll();
    const dt = forward ? 1 / 60 : -1 / 60;
    seekAll(currentTime + dt);
  }

  function startTickLoop() {
    stopTickLoop();
    function tick() {
      if (!isPlaying) return;
      if (players.length > 0) {
        const lead = players[0].el;
        if (lead && (lead._time != null || lead.currentTime != null)) {
          currentTime = lead._time ?? lead.currentTime ?? (currentTime + 1 / 60);
          if (currentTime >= maxDuration) {
            currentTime = 0;
            seekAll(0);
          } else {
            timelineSlider.value = currentTime;
            updateTimeDisplay();
          }
        }
      }
      rafId = requestAnimationFrame(tick);
    }
    rafId = requestAnimationFrame(tick);
  }

  function stopTickLoop() {
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
  }

  function updatePlayButton() {
    playBtn.textContent = isPlaying ? "⏸" : "▶";
    playBtn.title = isPlaying ? "Pause (Space)" : "Play (Space)";
  }

  function updateTimeDisplay() {
    timecodeDisplay.textContent = `${fmtTime(currentTime)} / ${fmtTime(maxDuration)}`;
    if (noteTimeInput) noteTimeInput.value = currentTime.toFixed(2);
  }

  // Decisions and Rubric Scoring
  function selectDecision(decision) {
    selectedDecision = decision;
    document.querySelectorAll(".decision-buttons .choice-btn").forEach((btn) => {
      btn.classList.toggle("selected", btn.dataset.decision === decision);
    });

    players.forEach((p) => {
      const isWinner = decision === `prefer_${p.id}`;
      p.card.classList.toggle("winner", isWinner);
    });

    // Auto-reveal in blind mode once decision is made
    if (activeMode === "blind") {
      revealContestants();
    }
  }

  function resetDecisionButtons() {
    document.querySelectorAll(".decision-buttons .choice-btn").forEach((b) => b.classList.remove("selected"));
    players.forEach((p) => p.card.classList.remove("winner"));
  }

  function revealContestants() {
    if (!currentBattle || !currentBattle.variants) return;
    currentBattle.variants.forEach((v, i) => {
      const labelEl = document.getElementById(`variant-label-${v.id}`);
      if (labelEl) {
        labelEl.textContent = `${v.label || 'Variant ' + v.id.toUpperCase()}`;
      }
      const codeSec = document.getElementById(`code-section-${v.id}`);
      if (codeSec) {
        codeSec.style.display = "flex";
      }
    });
  }

  // Rubric Star Ratings
  document.querySelectorAll(".star-rating").forEach((ratingEl) => {
    const axis = ratingEl.dataset.axis;
    const buttons = ratingEl.querySelectorAll("button");
    buttons.forEach((btn) => {
      btn.addEventListener("click", () => {
        const val = parseInt(btn.dataset.val, 10);
        rubricScores[axis] = val;
        buttons.forEach((b) => {
          const bVal = parseInt(b.dataset.val, 10);
          b.classList.toggle("active", bVal <= val);
        });
      });
    });
  });

  // Timeline Notes
  document.getElementById("add-note-btn").addEventListener("click", () => {
    const textEl = document.getElementById("note-text-input");
    const tagEl = document.getElementById("note-tag-select");
    const noteText = textEl.value.trim();
    if (!noteText) return;

    const note = {
      id: crypto.randomUUID(),
      time_ms: Math.round(parseFloat(noteTimeInput.value || currentTime) * 1000),
      tag: tagEl.value,
      note: noteText
    };

    timelineNotes.push(note);
    textEl.value = "";
    renderNotesList();
  });

  function renderNotesList() {
    const listEl = document.getElementById("notes-list");
    listEl.innerHTML = "";
    timelineNotes.forEach((n, idx) => {
      const row = document.createElement("div");
      row.className = "note-row";
      row.innerHTML = `
        <div class="note-meta">
          <span class="note-time">${(n.time_ms / 1000).toFixed(2)}s</span>
          <span class="note-tag">${n.tag}</span>
          <span>${n.note.replace(/[<>&]/g, (s) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;" }[s]))}</span>
        </div>
        <button class="note-delete" title="Delete note">✕</button>
      `;
      row.querySelector(".note-delete").addEventListener("click", () => {
        timelineNotes.splice(idx, 1);
        renderNotesList();
      });
      listEl.appendChild(row);
    });
  }

  // Submit Decision & Save Feedback
  document.getElementById("submit-decision-btn").addEventListener("click", async () => {
    if (!selectedDecision) {
      alert("Please select a decision (Prefer A, Prefer B, Tie, etc.) before submitting.");
      return;
    }

    const winnerId = selectedDecision.startsWith("prefer_")
      ? selectedDecision.replace("prefer_", "")
      : null;

    const payload = {
      battle_id: currentBattle.id,
      timestamp: new Date().toISOString(),
      decision: selectedDecision,
      winner: winnerId,
      rubric: rubricScores,
      notes: timelineNotes,
      outcome_summary: document.getElementById("outcome-summary-input").value.trim()
    };

    submitStatus.textContent = "Saving feedback...";
    try {
      const res = await fetch("/api/arena/vote", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload)
      });
      if (res.ok) {
        submitStatus.textContent = "✓ Saved to feedback.json and feedback.jsonl!";
        submitStatus.style.color = "var(--ok)";
      } else {
        throw new Error(`Server returned ${res.status}`);
      }
    } catch (_) {
      // Fallback: trigger download
      const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `feedback-${currentBattle.id}.json`;
      a.click();
      URL.revokeObjectURL(url);
      submitStatus.textContent = "✓ Exported feedback JSON file.";
      submitStatus.style.color = "var(--ok)";
    }
  });

  // UI Event Bindings
  battleSelect.addEventListener("change", (e) => loadBattle(e.target.value));

  document.querySelectorAll(".mode-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".mode-btn").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      activeMode = btn.dataset.mode;
      renderStage();
    });
  });

  playBtn.addEventListener("click", togglePlay);
  document.getElementById("master-step-back-btn").addEventListener("click", () => stepFrame(false));
  document.getElementById("master-step-fwd-btn").addEventListener("click", () => stepFrame(true));

  timelineSlider.addEventListener("input", (e) => {
    pauseAll();
    seekAll(parseFloat(e.target.value));
  });

  document.querySelectorAll(".speed-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".speed-btn").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      playbackSpeed = parseFloat(btn.dataset.speed);
    });
  });

  document.querySelectorAll(".decision-buttons .choice-btn").forEach((btn) => {
    btn.addEventListener("click", () => selectDecision(btn.dataset.decision));
  });

  // Keyboard Shortcuts
  window.addEventListener("keydown", (e) => {
    if (["INPUT", "TEXTAREA"].includes(document.activeElement?.tagName)) return;
    if (e.code === "Space") {
      e.preventDefault();
      togglePlay();
    } else if (e.code === "ArrowLeft") {
      e.preventDefault();
      stepFrame(false);
    } else if (e.code === "ArrowRight") {
      e.preventDefault();
      stepFrame(true);
    }
  });

  // Initialize
  loadBattleList();
})();
