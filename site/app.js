/* Map Generator — small, dependency-free interactions. */
(() => {
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  /* Sticky nav border */
  const nav = document.getElementById('nav');
  if (nav) {
    const onScroll = () => nav.classList.toggle('scrolled', window.scrollY > 8);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
  }

  /* Latest release: point every download button at the installer. */
  const releaseApi = 'https://api.github.com/repos/hannogeo/map-generator/releases/latest';
  fetch(releaseApi)
    .then((response) => response.json())
    .then((data) => {
      const assets = (data && data.assets) || [];
      const setup = assets.find((asset) => /-setup\.exe$/i.test(asset.name));
      const msi = assets.find((asset) => /\.msi$/i.test(asset.name));
      const pick = setup || msi;
      if (pick && pick.browser_download_url) {
        document.querySelectorAll('.js-download').forEach((link) => {
          link.href = pick.browser_download_url;
        });
      }
    })
    .catch(() => { /* keep the GitHub releases fallback link */ });

  /* Cursor helpers */
  function moveCursor(cursor, target) {
    if (!cursor || !target) return;
    const parent = cursor.offsetParent || cursor.parentElement;
    if (!parent) return;
    const base = parent.getBoundingClientRect();
    const rect = target.getBoundingClientRect();
    const x = rect.left - base.left + rect.width * 0.7;
    const y = rect.top - base.top + rect.height * 0.6;
    cursor.style.transform = `translate(${x}px, ${y}px)`;
  }

  function parkCursor(cursor) {
    if (!cursor) return;
    const parent = cursor.offsetParent || cursor.parentElement;
    if (!parent) return;
    const base = parent.getBoundingClientRect();
    cursor.style.transform = `translate(${base.width * 0.62}px, ${base.height * 0.78}px)`;
  }

  function click(cursor) {
    if (!cursor) return;
    const ring = cursor.querySelector('[data-click]');
    if (!ring) return;
    ring.classList.remove('ping');
    void ring.offsetWidth;
    ring.classList.add('ping');
  }

  function tickingTimer(el) {
    if (!el) return () => {};
    const started = performance.now();
    const update = () => {
      el.textContent = ((performance.now() - started) / 1000).toFixed(1) + 's';
    };
    update();
    const id = window.setInterval(update, 100);
    return () => window.clearInterval(id);
  }

  /* Hero: click Start, then reveal locations. */
  async function runHero(root) {
    const cursor = root.querySelector('[data-cursor]');
    const start = root.querySelector('[data-start]');
    const running = root.querySelector('[data-running]');
    const pins = Array.from(root.querySelectorAll('.pin'));
    const count = root.querySelector('[data-count]');
    const found = root.querySelector('[data-found]');
    const exported = root.querySelector('[data-export-count]');
    const timer = root.querySelector('[data-timer]');
    const target = 48;

    const settle = () => {
      pins.forEach((pin) => pin.classList.add('show'));
      if (count) count.textContent = target;
      if (found) found.textContent = target;
      if (exported) exported.textContent = target;
      if (running) running.classList.add('show');
      if (timer) timer.textContent = '11.8s';
      if (start) start.textContent = 'Stop generation';
    };

    if (reduceMotion || !cursor) {
      settle();
      return;
    }

    parkCursor(cursor);
    await sleep(300);
    cursor.classList.add('show');
    await sleep(250);

    if (start && start.offsetParent !== null) {
      moveCursor(cursor, start);
      await sleep(1050);
      click(cursor);
      start.classList.add('pressed');
      await sleep(130);
      start.classList.remove('pressed');
      start.textContent = 'Stop generation';
      await sleep(200);
    }

    if (running) running.classList.add('show');
    tickingTimer(timer);
    await sleep(250);

    for (let i = 0; i < pins.length; i += 1) {
      pins[i].classList.add('show');
      const n = Math.round(((i + 1) / pins.length) * target);
      if (count) count.textContent = n;
      if (found) found.textContent = n;
      if (exported) exported.textContent = n;
      await sleep(60);
    }

    if (count) count.textContent = target;
    if (found) found.textContent = target;
    if (exported) exported.textContent = target;
  }

  /* Selecting a country from search. */
  async function runSelect(root) {
    const cursor = root.querySelector('[data-cursor]');
    const result = root.querySelector('[data-result]');
    const selected = root.querySelector('.mini-map .sel');

    if (reduceMotion || !cursor) {
      if (result) result.classList.add('hot');
      if (selected) selected.classList.add('on');
      return;
    }

    parkCursor(cursor);
    await sleep(250);
    cursor.classList.add('show');
    await sleep(350);

    if (result) {
      moveCursor(cursor, result);
      await sleep(950);
      click(cursor);
      result.classList.add('hot');
      if (selected) selected.classList.add('on');
    }
  }

  /* Toggling a coverage filter. */
  async function runFilter(root) {
    const cursor = root.querySelector('[data-cursor]');
    const toggle = root.querySelector('[data-toggle]');
    const box = toggle && toggle.querySelector('.box');
    const reveal = root.querySelector('[data-reveal]');

    if (reduceMotion || !cursor) {
      if (box) box.classList.remove('checked');
      if (reveal) reveal.classList.add('show');
      return;
    }

    parkCursor(cursor);
    await sleep(250);
    cursor.classList.add('show');
    await sleep(300);

    if (toggle) {
      moveCursor(cursor, toggle);
      await sleep(900);
      click(cursor);
      if (box) box.classList.remove('checked');
      if (reveal) reveal.classList.add('show');
    }
  }

  /* Progress, notification and Discord message. */
  async function runProgress(root) {
    const found = root.querySelector('[data-found2]');
    const bar = root.querySelector('[data-progress]');
    const timer = root.querySelector('[data-timer2]');
    const toast = root.querySelector('[data-toast]');
    const discord = root.querySelector('[data-discord]');
    const target = 148;

    if (reduceMotion) {
      if (found) found.textContent = target;
      if (bar) bar.style.width = '14.8%';
      if (timer) timer.textContent = '9.4s';
      if (toast) toast.classList.add('show');
      if (discord) discord.classList.add('show');
      return;
    }

    if (toast) toast.classList.add('show');
    tickingTimer(timer);
    await sleep(700);

    if (found) {
      const started = performance.now();
      const duration = 1700;
      const step = () => {
        const k = Math.min(1, (performance.now() - started) / duration);
        const n = Math.round(k * target);
        found.textContent = n;
        if (bar) bar.style.width = (k * 14.8).toFixed(1) + '%';
        if (k < 1) requestAnimationFrame(step);
      };
      step();
    }

    await sleep(1200);
    if (discord) discord.classList.add('show');
  }

  /* Export feedback. */
  async function runExport(root) {
    const cursor = root.querySelector('[data-cursor]');
    const button = root.querySelector('[data-export-btn]');
    const notice = root.querySelector('[data-export-notice]');

    if (reduceMotion || !cursor) {
      if (notice) notice.classList.add('show');
      return;
    }

    parkCursor(cursor);
    await sleep(250);
    cursor.classList.add('show');
    await sleep(300);

    if (button) {
      moveCursor(cursor, button);
      await sleep(900);
      click(cursor);
      if (notice) notice.classList.add('show');
    }
  }

  /* Performance bars. */
  function runPerf(root) {
    root.classList.add('in');
  }

  const runners = {
    hero: runHero,
    select: runSelect,
    filter: runFilter,
    progress: runProgress,
    export: runExport,
    perf: runPerf,
  };

  if ('IntersectionObserver' in window) {
    const observer = new IntersectionObserver((entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        observer.unobserve(entry.target);
        const run = runners[entry.target.dataset.animate];
        if (run) run(entry.target);
      });
    }, { threshold: 0.3, rootMargin: '0px 0px -40px 0px' });

    document.querySelectorAll('[data-animate]').forEach((el) => observer.observe(el));
  } else {
    document.querySelectorAll('[data-animate]').forEach((el) => {
      const run = runners[el.dataset.animate];
      if (run) run(el);
    });
  }
})();
