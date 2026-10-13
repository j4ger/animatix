// deck.js — Interactive Slideshow Presentation Engine for Animatix Walkthroughs
// Supports: Deck Mode (slideshow), Article Mode (continuous scroll), keyboard
// navigation, ambient aura synchronisation, deep-linking, and lazy player play.

(() => {
  'use strict';

  class AnimatixDeck {
    constructor(root) {
      this.root = root;
      this.slides = Array.from(root.querySelectorAll('.deck-slide, section.chapter'));
      if (this.slides.length === 0) return;

      this.currentIndex = 0;
      this.mode = localStorage.getItem('amx-deck-view-mode') || 'deck'; // 'deck' | 'article'
      this.isAutoAdvancing = false;
      this.autoTimer = null;

      this.initDOM();
      this.bindEvents();
      this.applyInitialState();
    }

    initDOM() {
      this.root.classList.add('amx-deck-initialized');

      // Create or find controls container
      let nav = this.root.querySelector('.deck-stepper-bar');
      if (!nav) {
        nav = document.createElement('nav');
        nav.className = 'deck-stepper-bar';
        nav.setAttribute('aria-label', 'Deck Navigation');
        this.root.insertBefore(nav, this.slides[0]);
      }
      this.nav = nav;

      this.renderNav();
    }

    renderNav() {
      const total = this.slides.length;
      const totalStr = total.toString().padStart(2, '0');

      let tabsHtml = this.slides.map((slide, idx) => {
        const title = slide.dataset.title || slide.querySelector('h2, .stage-meta-kicker')?.textContent?.trim() || `Figure ${idx + 1}`;
        const numStr = idx.toString().padStart(2, '0');
        return `
          <button type="button" class="deck-pill ${idx === this.currentIndex ? 'active' : ''}" data-index="${idx}" title="${title}">
            <span class="pill-num">${numStr}</span>
            <span class="pill-label">${title}</span>
          </button>
        `;
      }).join('');

      this.nav.innerHTML = `
        <div class="deck-nav-strip">
          <div class="deck-nav-meta">
            <span class="deck-mode-badge">
              <span class="live-dot"></span>
              <span class="deck-count-text"><b class="cur-slide-num">01</b> / ${totalStr}</span>
            </span>
            <div class="deck-view-toggles" role="group" aria-label="View Mode">
              <button type="button" class="btn-deck-mode ${this.mode === 'deck' ? 'active' : ''}" data-mode="deck" title="Slideshow Presentation Deck">
                <span class="deck-mode-icon">⊞</span>
                <span class="deck-mode-text">Deck</span>
              </button>
              <button type="button" class="btn-deck-mode ${this.mode === 'article' ? 'active' : ''}" data-mode="article" title="Continuous Reading Article">
                <span class="deck-mode-icon">☰</span>
                <span class="deck-mode-text">Article</span>
              </button>
            </div>
          </div>

          <div class="deck-pills-scroll" role="tablist">
            ${tabsHtml}
          </div>

          <div class="deck-nav-actions">
            <button type="button" class="deck-arrow-btn deck-prev" aria-label="Previous Slide (Left Arrow)" title="Previous (←)">
              <span>←</span>
            </button>
            <button type="button" class="deck-arrow-btn deck-next" aria-label="Next Slide (Right Arrow)" title="Next (→)">
              <span>→</span>
            </button>
          </div>
        </div>
      `;
    }

    bindEvents() {
      // Pill click
      this.nav.addEventListener('click', (e) => {
        const pill = e.target.closest('.deck-pill');
        if (pill) {
          const idx = parseInt(pill.dataset.index, 10);
          if (!isNaN(idx)) this.goToSlide(idx);
          return;
        }

        const modeBtn = e.target.closest('.btn-deck-mode');
        if (modeBtn) {
          const newMode = modeBtn.dataset.mode;
          if (newMode) this.setMode(newMode);
          return;
        }

        if (e.target.closest('.deck-prev')) {
          this.prev();
          return;
        }

        if (e.target.closest('.deck-next')) {
          this.next();
          return;
        }
      });

      // Keyboard navigation when page/window is active (and not focusing an input)
      window.addEventListener('keydown', (e) => {
        if (['INPUT', 'TEXTAREA'].includes(document.activeElement?.tagName)) return;
        if (this.mode !== 'deck') return;

        if (e.key === 'ArrowRight' || e.key === 'l') {
          e.preventDefault();
          this.next();
        } else if (e.key === 'ArrowLeft' || e.key === 'h') {
          e.preventDefault();
          this.prev();
        } else if (e.key === ' ') {
          // Space toggles playback on active player
          const activeSlide = this.slides[this.currentIndex];
          const player = activeSlide?.querySelector('amx-player');
          if (player) {
            e.preventDefault();
            if (typeof player.togglePlay === 'function') {
              player.togglePlay();
            } else if (player.hasAttribute('autoplay')) {
              // Toggle sealed/autoplay if custom method not exported
              player.dispatchEvent(new CustomEvent('toggle-play'));
            }
          }
        }
      });

      // Hash navigation listener
      window.addEventListener('hashchange', () => {
        this.parseHash();
      });
    }

    applyInitialState() {
      this.parseHash();
      this.updateViewModeDOM();
      this.updateSlideDOM();
    }

    parseHash() {
      const hash = window.location.hash.replace(/^#/, '');
      if (!hash) return;

      // Match #slide=N or #sN
      const slideMatch = hash.match(/^(?:slide=)?(\d+)$/);
      if (slideMatch) {
        const idx = parseInt(slideMatch[1], 10);
        if (idx >= 0 && idx < this.slides.length) {
          this.currentIndex = idx;
          this.updateSlideDOM();
          return;
        }
      }

      // Match by ID
      const targetIdx = this.slides.findIndex(s => s.id === hash || s.dataset.slideId === hash);
      if (targetIdx !== -1) {
        this.currentIndex = targetIdx;
        this.updateSlideDOM();
      }
    }

    setMode(mode) {
      if (this.mode === mode) return;
      this.mode = mode;
      localStorage.setItem('amx-deck-view-mode', mode);
      this.updateViewModeDOM();

      if (mode === 'article') {
        // Scroll active slide into view
        const activeSlide = this.slides[this.currentIndex];
        activeSlide?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    }

    updateViewModeDOM() {
      this.root.classList.remove('deck-mode-slideshow', 'deck-mode-article');
      this.root.classList.add(this.mode === 'deck' ? 'deck-mode-slideshow' : 'deck-mode-article');

      this.nav.querySelectorAll('.btn-deck-mode').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.mode === this.mode);
      });
    }

    goToSlide(index) {
      if (index < 0 || index >= this.slides.length) return;
      if (this.currentIndex === index && this.mode === 'deck') return;

      this.currentIndex = index;
      this.updateSlideDOM();

      // Deep link without triggering browser jump
      const slideId = this.slides[index].id || `slide=${index}`;
      history.replaceState(null, '', `#${slideId}`);

      if (this.mode === 'article') {
        this.slides[index].scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    }

    next() {
      if (this.currentIndex < this.slides.length - 1) {
        this.goToSlide(this.currentIndex + 1);
      } else {
        // Wrap to first
        this.goToSlide(0);
      }
    }

    prev() {
      if (this.currentIndex > 0) {
        this.goToSlide(this.currentIndex - 1);
      } else {
        // Wrap to last
        this.goToSlide(this.slides.length - 1);
      }
    }

    updateSlideDOM() {
      const activeSlide = this.slides[this.currentIndex];
      if (!activeSlide) return;

      // Update pills
      this.nav.querySelectorAll('.deck-pill').forEach((pill, idx) => {
        const isActive = idx === this.currentIndex;
        pill.classList.toggle('active', isActive);
        pill.setAttribute('aria-selected', isActive ? 'true' : 'false');
        if (isActive) {
          pill.scrollIntoView({ behavior: 'smooth', inline: 'nearest', block: 'nearest' });
        }
      });

      // Update counter
      const counterEl = this.nav.querySelector('.cur-slide-num');
      if (counterEl) {
        counterEl.textContent = (this.currentIndex + 1).toString().padStart(2, '0');
      }

      // Update Slides visibility in deck mode
      this.slides.forEach((slide, idx) => {
        const isActive = idx === this.currentIndex;
        slide.classList.toggle('active-slide', isActive);
        if (this.mode === 'deck') {
          slide.style.display = isActive ? '' : 'none';
        } else {
          slide.style.display = '';
        }

        // Auto-pause inactive players, resume active
        const player = slide.querySelector('amx-player');
        if (player) {
          if (!isActive && this.mode === 'deck') {
            player.pause?.();
          } else if (isActive) {
            player.play?.();
          }
        }
      });

      // Update theme accent glow on root
      const accent = activeSlide.dataset.accent || '#f5b942';
      this.root.style.setProperty('--deck-accent', accent);
    }
  }

  // Auto-initialize all decks on DOMContentLoaded
  function init() {
    document.querySelectorAll('[data-amx-deck], .walkthrough-deck').forEach(el => {
      new AnimatixDeck(el);
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
