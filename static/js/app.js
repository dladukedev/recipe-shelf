(() => {
  document.documentElement.classList.add('js');
  const read = key => { try { return JSON.parse(sessionStorage.getItem(key) || 'null'); } catch { return null; } };
  const write = (key, value) => { try { sessionStorage.setItem(key, JSON.stringify(value)); return true; } catch { return false; } };
  const remove = key => { try { sessionStorage.removeItem(key); } catch {} };
  let leavingInternally = false, volatileDrafts = false;
  let hasUnsaved = () => false, discardDrafts = () => {}, resetRecipeSelection = () => {};
  const markTemplateNavigation = () => {
    const nav = document.querySelector('.settings-tabs');
    const link = nav?.querySelector('a[href="/settings/templates"]');
    if (link) link.textContent = `Templates${Object.keys(read(`recipe-shelf:templates:${nav.dataset.owner}`) || {}).length ? ' *' : ''}`;
  };
  const allowDeparture = () => {
    if (hasUnsaved()) {
      if (!confirm('You have unsaved template changes. Leave Templates and lose these changes?')) return false;
      discardDrafts();
    }
    resetRecipeSelection();
    return true;
  };
  window.addEventListener('beforeunload', event => {
    if (hasUnsaved() && !leavingInternally) { event.preventDefault(); event.returnValue = ''; }
  });
  window.addEventListener('pagehide', () => {
    // A native unload/reload confirmation was accepted. Internal format changes
    // and template submissions preserve drafts until the server confirms a save.
    if (!leavingInternally && hasUnsaved()) discardDrafts();
  });
  window.addEventListener('pageshow', () => { leavingInternally = false; });
  document.querySelectorAll('[data-snackbar]').forEach(snackbar => {
    let timer;
    const pause = () => clearTimeout(timer);
    const resume = () => { pause(); timer = setTimeout(() => snackbar.remove(), 7000); };
    snackbar.addEventListener('mouseenter', pause);
    snackbar.addEventListener('mouseleave', resume);
    snackbar.addEventListener('focusin', pause);
    snackbar.addEventListener('focusout', resume);
    snackbar.querySelector('[data-snackbar-dismiss]')?.addEventListener('click', () => { pause(); snackbar.remove(); });
    resume();
  });
  const activateDialog = (dialog, opener) => {
    if (typeof dialog.showModal !== 'function') return;
    if (dialog.open) dialog.removeAttribute('open');
    dialog.showModal();
    dialog.addEventListener('close', () => {
      opener?.focus();
      const url = new URL(location.href);
      for (const key of ['modal', 'edit', 'add', 'error']) url.searchParams.delete(key);
      history.replaceState(null, '', url.pathname + url.search);
    }, {once: true});
  };
  document.querySelectorAll('dialog[data-modal][open]').forEach(dialog => {
    activateDialog(dialog, document.querySelector(`[data-modal-link="${dialog.id}"]`));
  });
  let modalRequest;
  const openModalRoute = async link => {
    modalRequest?.abort();
    const controller = new AbortController(); modalRequest = controller;
    try {
      const response = await fetch(link.href, {signal: controller.signal});
      if (!response.ok) throw new Error('Dialog unavailable');
      const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
      const dialog = doc.getElementById(link.dataset.modalLink);
      if (!dialog || typeof dialog.showModal !== 'function') { location.assign(response.url || link.href); return; }
      if (controller.signal.aborted) return;
      document.getElementById(dialog.id)?.remove();
      document.getElementById('main').append(dialog);
      activateDialog(dialog, link);
    } catch (error) { if (error.name !== 'AbortError') location.assign(link.href); }
  };
  const keyboardDestination = read('recipe-shelf:focus-heading');
  if (keyboardDestination === location.pathname + location.search) {
    const heading = document.querySelector('h1') || document.getElementById('main');
    if (heading) { heading.tabIndex = -1; heading.focus({preventScroll: true}); }
    remove('recipe-shelf:focus-heading');
  }
  markTemplateNavigation();
  const listKey = () => `recipe-shelf:list:${document.getElementById('recipe-results')?.dataset.listUrl || location.pathname + location.search}`;

  document.addEventListener('click', event => {
    const close = event.target.closest('[data-modal-close]');
    if (close && typeof close.closest('dialog')?.close === 'function') {
      event.preventDefault(); close.closest('dialog').close(); return;
    }
    const link = event.target.closest('a[href]');
    if (link && !event.defaultPrevented && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey && event.button === 0 && link.target !== '_blank') {
      const destination = new URL(link.href);
      const download = /\/export\/|\/exports\/download/.test(destination.pathname) || link.hasAttribute('download');
      const leavingTemplates = destination.origin !== location.origin || destination.pathname !== '/settings/templates';
      if (!download && leavingTemplates && !allowDeparture()) { event.preventDefault(); return; }
      if (link.dataset.modalLink) {
        event.preventDefault(); leavingInternally = false; openModalRoute(link); return;
      }
      if (!download && !destination.hash) {
        leavingInternally = true;
        if (event.detail === 0) write('recipe-shelf:focus-heading', destination.pathname + destination.search);
      }
    }
    document.querySelectorAll('[data-menu][open]').forEach(menu => {
      if (!menu.contains(event.target) || event.target.closest('.menu-options a')) menu.open = false;
    });
    if (event.target.closest('[data-recipe-link]')) {
      write(listKey(), {scroll: window.scrollY, focus: event.target.closest('[data-recipe-link]').getAttribute('href')});
    }
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') document.querySelectorAll('[data-menu][open]').forEach(menu => { menu.open = false; menu.querySelector('summary').focus(); });
    if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.target.closest('input,textarea,select,[contenteditable="true"]')) {
      const search = document.getElementById('recipe-search');
      if (search) { event.preventDefault(); search.focus(); }
    }
  });
  document.addEventListener('submit', event => {
    const form = event.target;
    if (form.dataset.confirm && !confirm(form.dataset.confirm)) { event.preventDefault(); return; }
    if (form.matches('[data-password-form]')) {
      const password = form.elements.password, confirmPassword = form.elements.confirm;
      confirmPassword.setCustomValidity(password.value === confirmPassword.value ? '' : 'The new passwords do not match.');
      if (!form.reportValidity()) event.preventDefault();
    }
    if (event.defaultPrevented) return;
    const action = new URL(event.submitter?.formAction || form.action || location.href);
    const download = /\/export\/|\/exports\/download/.test(action.pathname);
    if (!form.matches('[data-template-form]') && !download && action.pathname !== '/settings/templates' && !allowDeparture()) { event.preventDefault(); return; }
    if (!download) leavingInternally = true;
    if (form.action && new URL(form.action).pathname === '/logout') {
      try { Object.keys(sessionStorage).filter(key => key.startsWith('recipe-shelf:')).forEach(remove); } catch {}
    }
  });
  document.addEventListener('input', event => {
    event.target.closest('[data-password-form]')?.elements.confirm.setCustomValidity('');
  });

  document.querySelector('.skip-link')?.addEventListener('click', event => { event.preventDefault(); document.getElementById('main').focus(); });

  const controls = document.getElementById('recipe-filters');
  if (controls) {
    let pending, timer;
    const refresh = async submitter => {
      pending?.abort();
      const controller = new AbortController(); pending = controller;
      const data = new FormData(controls);
      data.set('category', submitter?.name === 'category' ? submitter.value : (new URLSearchParams(location.search).get('category') || ''));
      const url = `/?${new URLSearchParams(data)}`;
      document.getElementById('recipe-results').setAttribute('aria-busy', 'true');
      try {
        const response = await fetch(url, {signal: controller.signal});
        if (!response.ok) throw new Error('Search failed');
        const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
        if (controller.signal.aborted) return;
        const results = doc.getElementById('recipe-results');
        if (!results) { location.assign(response.url); return; }
        document.getElementById('recipe-results').replaceWith(results);
        history.replaceState(null, '', results.dataset.listUrl);
        controls.querySelectorAll('.chip').forEach(button => button.setAttribute('aria-pressed', String(button.value === data.get('category'))));
        controls.querySelector('.search-submit').value = data.get('category');
        document.getElementById('results-announcement').textContent = results.querySelector('.results-meta').textContent.trim();
      } catch (error) {
        if (error.name !== 'AbortError') location.assign(url);
      } finally {
        if (pending === controller) document.getElementById('recipe-results')?.removeAttribute('aria-busy');
      }
    };
    controls.addEventListener('submit', event => { event.preventDefault(); clearTimeout(timer); refresh(event.submitter); });
    controls.elements.q.addEventListener('input', () => { pending?.abort(); clearTimeout(timer); timer = setTimeout(() => refresh(), 250); });
    controls.elements.sort.addEventListener('change', () => { clearTimeout(timer); refresh(); });
    const restore = () => {
      const state = read(listKey());
      if (!state || !Number.isFinite(state.scroll)) return;
      requestAnimationFrame(() => {
        [...document.querySelectorAll('[data-recipe-link]')].find(link => link.getAttribute('href') === state.focus)?.focus({preventScroll: true});
        window.scrollTo(0, state.scroll);
      });
    };
    window.addEventListener('pageshow', restore);
  }

  const collection = document.getElementById('collection-export');
  if (collection) {
    let request;
    const refreshPreview = async () => {
      request?.abort();
      const controller = new AbortController(); request = controller;
      const params = new URLSearchParams(new FormData(collection));
      const format = params.get('format');
      const customize = document.getElementById('customize-export-template');
      if (customize) customize.href = `/settings/templates?${new URLSearchParams({format})}`;
      const output = document.getElementById('export-preview');
      const message = document.getElementById('export-preview-message');
      output.setAttribute('aria-busy', 'true');
      try {
        const response = await fetch(`/exports/preview?${params}`, {signal: controller.signal});
        if (!response.ok) throw new Error('Preview unavailable. Try updating the preview again.');
        const result = await response.json();
        if (controller.signal.aborted) return;
        output.textContent = result.preview || '';
        message.textContent = result.error || (result.truncated ? 'Preview shortened. Exports use the full template.' : format === 'pdf' ? 'PDF preview shows the template’s HTML.' : 'Preview using your saved template.');
        history.replaceState(null, '', `/exports?${params}`);
      } catch (error) { if (error.name !== 'AbortError') message.textContent = error.message; }
      finally { if (request === controller) output.removeAttribute('aria-busy'); }
    };
    collection.addEventListener('change', refreshPreview);
    document.getElementById('export-preview-recipe')?.addEventListener('change', refreshPreview);
    document.getElementById('export-update-preview')?.addEventListener('click', event => { event.preventDefault(); refreshPreview(); });
  }

  const template = document.querySelector('[data-template-form]');
  if (template) {
    const editor = template.elements.source;
    const saved = editor.dataset.savedSource ?? editor.value;
    const format = template.elements.format.value;
    const key = `recipe-shelf:templates:${template.dataset.owner}`;
    const drafts = read(key) || {};
    let discarded = false;
    hasUnsaved = () => Object.keys(drafts).length > 0;
    discardDrafts = () => {
      discarded = true;
      for (const format of Object.keys(drafts)) delete drafts[format];
      remove(key); volatileDrafts = false; markTemplateNavigation();
    };
    const recipeChoice = document.getElementById('template-preview-recipe');
    let resetRecipe = false;
    const syncRecipeSelection = (reset = false) => {
      if (!recipeChoice) return;
      document.querySelectorAll('.format-tabs a').forEach(link => {
        const url = new URL(link.href);
        if (reset) url.searchParams.delete('recipe');
        else url.searchParams.set('recipe', recipeChoice.value);
        link.href = url.href;
      });
      if (template.elements.recipe) template.elements.recipe.value = recipeChoice.value;
      const url = new URL(location.href);
      if (reset) url.searchParams.delete('recipe');
      else url.searchParams.set('recipe', recipeChoice.value);
      history.replaceState(null, '', url.pathname + url.search);
    };
    resetRecipeSelection = () => {
      if (!recipeChoice) return;
      recipeChoice.value = recipeChoice.dataset.defaultRecipe || recipeChoice.options?.[0]?.value || '';
      resetRecipe = true;
      syncRecipeSelection(true);
    };
    syncRecipeSelection();
    // A successful save removes only the draft that the server actually saved.
    if (drafts[format]?.source === saved) delete drafts[format];
    else if (typeof drafts[format]?.source === 'string') editor.value = drafts[format].source;
    const conflict = drafts[format] && drafts[format].base !== saved && drafts[format].source !== saved;
    let previewTimer, previewRequest;
    const preview = async () => {
      previewRequest?.abort();
      const controller = new AbortController(); previewRequest = controller;
      const output = document.getElementById('template-preview');
      if (!output) return;
      try {
        const data = new URLSearchParams(new FormData(template));
        if (recipeChoice?.value) data.set('recipe', recipeChoice.value);
        const response = await fetch('/settings/templates/preview', {method: 'POST', body: data, signal: controller.signal});
        if (!response.ok) throw new Error('Preview unavailable. Save the template and try an export.');
        const result = await response.json();
        if (controller.signal.aborted) return;
        output.textContent = result.preview || '';
        document.getElementById('preview-message').textContent = result.error || (result.truncated ? 'Preview shortened. Exports use the full template.' : 'Preview using the selected recipe.');
      } catch (error) { if (error.name !== 'AbortError') document.getElementById('preview-message').textContent = error.message; }
    };
    const status = () => {
      if (editor.value === saved) delete drafts[format];
      else drafts[format] = {source: editor.value, base: saved};
      const retained = write(key, drafts);
      volatileDrafts = !retained && Object.keys(drafts).length > 0;
      markTemplateNavigation();
      const dirty = Object.keys(drafts);
      document.getElementById('template-status').textContent = dirty.length ? `Unsaved changes * (${dirty.join(', ')})` : '';
      if (volatileDrafts) document.getElementById('template-status').textContent = 'Unsaved changes. Draft storage is unavailable; save before leaving this page.';
      if (conflict && dirty.includes(format)) document.getElementById('template-status').textContent += ' The saved version has changed since this draft. Review your edits before saving, or discard them.';
      document.querySelectorAll('.format-tabs a').forEach(link => {
        const kind = new URL(link.href).searchParams.get('format');
        link.dataset.label ||= link.textContent;
        link.textContent = `${link.dataset.label}${drafts[kind] ? ' *' : ''}`;
      });
    };
    editor.addEventListener('input', () => { status(); previewRequest?.abort(); clearTimeout(previewTimer); previewTimer = setTimeout(preview, 300); });
    template.querySelector('[data-template-reset]')?.addEventListener('click', () => { editor.value = editor.dataset.defaultSource; status(); preview(); });
    template.querySelector('[data-template-discard]')?.addEventListener('click', () => { editor.value = saved; status(); preview(); });
    template.addEventListener('submit', () => { status(); leavingInternally = true; volatileDrafts = false; });
    window.addEventListener('pageshow', () => {
      if (discarded) { editor.value = saved; discarded = false; status(); }
      if (resetRecipe) { resetRecipe = false; preview(); }
    });
    recipeChoice?.addEventListener('change', () => { syncRecipeSelection(); preview(); });
    document.getElementById('template-recipe-form')?.addEventListener('submit', event => { event.preventDefault(); preview(); });
    status(); preview();
  }
})();
