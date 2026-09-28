// DreamWeave Network: progressive enhancement. Every page works without this file; it adds
// search, the catalog filter and copy buttons. It reads only files shipped with the site
// (network-data/search.json) and never talks to a project's origin or anything else.
(() => {
  'use strict';

  const FIELDS = new Set([
    'type', 'status', 'tag', 'game', 'maintainer', 'license', 'channel', 'health', 'id', 'site',
    'runtime', 'lua', 'provides', 'requires', 'recommends', 'conflicts', 'compatible', 'replaces',
  ]);

  // Copy buttons ----------------------------------------------------------------------------

  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      const area = document.createElement('textarea');
      area.value = text;
      area.setAttribute('readonly', '');
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.append(area);
      area.select();
      const copied = document.execCommand('copy');
      area.remove();
      return copied;
    }
  }

  document.addEventListener('click', async event => {
    const button = event.target.closest('[data-copy], [data-copy-code]');
    if (!button) return;
    const text = button.hasAttribute('data-copy')
      ? button.getAttribute('data-copy')
      : button.parentElement.querySelector('code').textContent;
    const label = button.dataset.label || button.textContent;
    button.dataset.label = label;
    button.textContent = await copyText(text) ? 'Copied' : 'Copy failed';
    window.setTimeout(() => { button.textContent = label; }, 1400);
  });

  for (const code of document.querySelectorAll('pre > code')) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'dw-copy';
    button.textContent = 'Copy';
    button.setAttribute('data-copy-code', '');
    button.setAttribute('aria-label', 'Copy code to clipboard');
    code.parentElement.append(button);
  }

  // The query language: words match anywhere, field:value matches one field -----------------

  function parse(query) {
    const words = [];
    const fields = [];
    for (const token of query.toLocaleLowerCase().split(/\s+/).filter(Boolean)) {
      const colon = token.indexOf(':');
      const field = colon > 0 ? token.slice(0, colon) : '';
      if (FIELDS.has(field) && colon < token.length - 1) {
        fields.push([field, token.slice(colon + 1)]);
      } else {
        words.push(token);
      }
    }
    return { words, fields };
  }

  function matches(record, query) {
    const text = record.text || '';
    if (!query.words.every(word => text.includes(word))) return false;
    return query.fields.every(([field, value]) =>
      (record.fields?.[field] || []).some(entry => entry.includes(value)));
  }

  function score(record, query) {
    const title = (record.title || '').toLocaleLowerCase();
    let total = record.kind === 'project' ? 3 : 0;
    for (const word of query.words) {
      if (title.startsWith(word)) total += 10;
      else if (title.includes(word)) total += 5;
    }
    return total;
  }

  let records = null;
  function loadRecords(url) {
    records ??= fetch(url)
      .then(response => {
        if (!response.ok) throw new Error(`search data returned ${response.status}`);
        return response.json();
      })
      .then(data => data.records)
      .catch(error => {
        records = null;
        throw error;
      });
    return records;
  }

  // Header search ---------------------------------------------------------------------------

  const input = document.getElementById('net-search-input');
  if (input && input.dataset.search) {
    const results = document.getElementById('net-search-results');
    const status = results.querySelector('.dw-search__status');
    const list = results.querySelector('.dw-search__list');
    const base = input.dataset.base || '/';
    let request = 0;

    const render = async () => {
      const current = ++request;
      const text = input.value.trim();
      list.replaceChildren();
      results.hidden = !text;
      if (!text) return;
      status.textContent = 'Searching…';
      let all;
      try {
        all = await loadRecords(input.dataset.search);
      } catch {
        status.textContent = 'Search is unavailable here. Every page is still reachable from the navigation.';
        return;
      }
      if (current !== request) return;
      const query = parse(text);
      const found = all
        .filter(record => matches(record, query))
        .map(record => ({ record, score: score(record, query) }))
        .sort((left, right) => right.score - left.score || left.record.title.localeCompare(right.record.title))
        .slice(0, 12);
      status.textContent = found.length
        ? `${found.length} result${found.length === 1 ? '' : 's'}${found.length === 12 ? ' (first 12)' : ''}`
        : 'Nothing matches. Fewer words, or a field like type:library.';
      for (const { record } of found) {
        const item = document.createElement('li');
        const link = document.createElement('a');
        link.href = new URL(record.url, base).href;
        const title = document.createElement('strong');
        title.textContent = record.title;
        const badge = document.createElement('small');
        badge.textContent = record.badge;
        title.append(badge);
        const summary = document.createElement('span');
        summary.textContent = record.summary;
        link.append(title, summary);
        item.append(link);
        list.append(item);
      }
    };

    let timer = 0;
    input.addEventListener('input', () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(render, 110);
    });
    input.addEventListener('focus', () => loadRecords(input.dataset.search).catch(() => {}), { once: true });
    input.addEventListener('keydown', event => {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        list.querySelector('a')?.focus();
      } else if (event.key === 'Escape') {
        results.hidden = true;
        input.blur();
      } else if (event.key === 'Enter') {
        const first = list.querySelector('a');
        if (first) window.location.href = first.href;
      }
    });
    list.addEventListener('keydown', event => {
      const links = [...list.querySelectorAll('a')];
      const position = links.indexOf(document.activeElement);
      if (event.key === 'ArrowDown' && position < links.length - 1) {
        event.preventDefault();
        links[position + 1].focus();
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        (position > 0 ? links[position - 1] : input).focus();
      } else if (event.key === 'Escape') {
        results.hidden = true;
        input.focus();
      }
    });
    document.addEventListener('keydown', event => {
      const typing = ['INPUT', 'TEXTAREA', 'SELECT'].includes(document.activeElement?.tagName)
        || document.activeElement?.isContentEditable;
      if (event.key === '/' && !typing) {
        event.preventDefault();
        (document.getElementById('net-filter-text') || input).focus();
      }
    });
    document.addEventListener('click', event => {
      if (!event.target.closest('.net-search')) results.hidden = true;
    });
  }

  // The catalog filter: the same query language over the cards on the page -------------------

  const pathOf = url => url.pathname.replace(/\/+$/, '');

  const filter = document.querySelector('[data-catalog-filter]');
  const cards = [...document.querySelectorAll('.net-card')];
  if (filter && cards.length && input?.dataset.search) {
    const field = filter.querySelector('input');
    const status = filter.querySelector('.net-filter__status');
    const empty = document.querySelector('[data-catalog-empty]');
    const base = input.dataset.base || '/';
    let byPath = null;

    const apply = async () => {
      const text = field.value.trim();
      if (byPath === null) {
        try {
          const all = await loadRecords(input.dataset.search);
          byPath = new Map(all.map(record => [pathOf(new URL(record.url, base)), record]));
        } catch {
          byPath = new Map();
        }
      }
      const query = parse(text);
      let shown = 0;
      for (const card of cards) {
        const link = card.querySelector('.net-card__title a');
        const record = link && byPath.get(pathOf(new URL(link.href)));
        const visible = !text || (record
          ? matches(record, query)
          : query.fields.length === 0 && query.words.every(word => card.textContent.toLocaleLowerCase().includes(word)));
        card.hidden = !visible;
        if (visible) shown += 1;
      }
      status.textContent = text ? `${shown} of ${cards.length} shown` : '';
      if (empty) empty.hidden = shown > 0;
      const url = new URL(window.location.href);
      if (text) url.searchParams.set('q', text); else url.searchParams.delete('q');
      window.history.replaceState(null, '', url);
    };

    filter.addEventListener('submit', event => event.preventDefault());
    let timer = 0;
    field.addEventListener('input', () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(apply, 90);
    });
    const initial = new URL(window.location.href).searchParams.get('q');
    if (initial) {
      field.value = initial;
      apply();
    }
  }
})();
