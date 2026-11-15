// Page-local conveniences; deck.js remains the single parameter rebuild path.
(() => {
  function init() {
    const root = document.querySelector('.gradient-page');
    const input = root?.querySelector('input[data-var="lr"]');
    const summary = root?.querySelector('#rate-summary');
    if (!input || !summary) return;
    const buttons = root.querySelectorAll('button[data-rate]');
    const update = () => {
      const rate = Number(input.value);
      const at = `At ${rate.toFixed(2)}, `;
      summary.textContent = at + (rate >= 0.5
        ? 'x repeats a two-cycle without shrinking. Loss does not reach zero.'
        : rate > 0.25
          ? 'x changes sides on each update, but loss keeps falling toward zero.'
          : 'loss tends to zero without either coordinate changing sides.');
      buttons.forEach((button) => {
        button.setAttribute('aria-pressed', String(Math.abs(Number(button.dataset.rate) - rate) < 1e-9));
      });
    };
    buttons.forEach((button) => {
      button.addEventListener('click', () => {
        input.value = button.dataset.rate;
        // Same event as moving the slider: no second setVars implementation.
        input.dispatchEvent(new Event('input', { bubbles: true }));
      });
    });
    input.addEventListener('input', update);
    update();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
