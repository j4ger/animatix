const begin = `<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<script type="module" src="https://amx.j4ger.dev/runtime/amx-player.js"></script>`;

export const embedExample = `${begin}

<amx-player src="./scrub.amx" title="Moving tile" controls></amx-player>`;

export const cueExample = `${begin}

<amx-player id="tile" src="./scrub.amx" title="Moving tile" sealed eager>
</amx-player>
<button type="button" data-cue="start" disabled>Show start</button>
<button type="button" data-cue="middle" disabled>Show middle</button>
<p id="status" role="status"></p>

<script>
  const player = document.querySelector('#tile');
  const buttons = document.querySelectorAll('[data-cue]');
  const status = document.querySelector('#status');
  player.addEventListener('amxready', () => {
    status.textContent = '';
    for (const button of buttons) button.disabled = false;
  });
  player.addEventListener('amxerror', ({ detail }) => {
    for (const button of buttons) button.disabled = true;
    status.textContent = detail.message;
  });
  for (const button of buttons) {
    button.addEventListener('click', () => {
      const cue = player.timelineMarkers.find(marker =>
        marker.kind === 'cue' && marker.id === button.dataset.cue);
      if (!cue) {
        status.textContent = 'Cue not found: ' + button.dataset.cue;
        return;
      }
      status.textContent = '';
      player.pause();
      player.seek(cue.t);
    });
  }
</script>`;

export const presentationExample = `${begin}

<amx-player src="./scrub.amx" title="Moving tile" presentation controls>
</amx-player>`;
