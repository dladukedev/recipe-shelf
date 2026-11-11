(() => {
  const key = 'recipe-shelf-theme';
  const customKey = 'recipe-shelf-custom-theme';
  const modes = ['light', 'dark'];
  const colors = ['foreground', 'background', 'accent', 'danger'];
  const media = matchMedia('(prefers-color-scheme: dark)');
  const root = document.documentElement;
  const emptyCustom = () => ({enabled: false, light: {}, dark: {}});
  const validColor = value => typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
  const parseCustom = value => {
    const result = emptyCustom();
    try {
      const saved = JSON.parse(value);
      result.enabled = saved?.enabled === true;
      for (const mode of modes) {
        for (const color of colors) {
          const value = saved?.[mode]?.[color];
          if (validColor(value)) result[mode][color] = value.toLowerCase();
        }
      }
    } catch {}
    return result;
  };
  let choice = 'system';
  let custom = emptyCustom();
  try {
    choice = localStorage.getItem(key) || 'system';
    custom = parseCustom(localStorage.getItem(customKey));
  } catch {}
  if (!['light', 'dark', 'system'].includes(choice)) choice = 'system';
  const fields = () => document.querySelectorAll('[data-custom-theme-color]');
  const syncFields = () => {
    fields().forEach(input => {
      input.value = custom[input.dataset.customThemeMode][input.dataset.customThemeColor] || input.defaultValue;
      input.setCustomValidity('');
      input.removeAttribute('aria-invalid');
    });
  };
  const apply = () => {
    const mode = choice === 'system' ? (media.matches ? 'dark' : 'light') : choice;
    root.dataset.theme = mode;
    root.dataset.themeChoice = choice;
    for (const color of colors) {
      if (custom.enabled && custom[mode][color]) root.style.setProperty(`--${color}`, custom[mode][color]);
      else root.style.removeProperty(`--${color}`);
    }
    document.querySelectorAll('button[data-theme-choice]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.themeChoice === choice)));
    const toggle = document.querySelector('[data-custom-theme-toggle]');
    if (toggle) toggle.checked = custom.enabled;
    const editor = document.querySelector('[data-custom-theme-editor]');
    if (editor) editor.hidden = !custom.enabled;
  };
  const saveCustom = () => {
    try { localStorage.setItem(customKey, JSON.stringify(custom)); } catch {}
  };
  apply();
  document.addEventListener('DOMContentLoaded', () => { apply(); syncFields(); });
  media.addEventListener('change', apply);
  window.addEventListener('storage', event => {
    if (event.key === key || event.key === null) {
      choice = ['light', 'dark'].includes(event.newValue) ? event.newValue : 'system';
    }
    if (event.key === customKey || event.key === null) custom = parseCustom(event.newValue);
    apply();
    syncFields();
  });
  document.addEventListener('change', event => {
    if (event.target.dataset?.customThemeToggle === undefined) return;
    custom.enabled = event.target.checked;
    saveCustom();
    apply();
    syncFields();
  });
  document.addEventListener('input', event => {
    const input = event.target;
    const mode = input.dataset?.customThemeMode;
    const color = input.dataset?.customThemeColor;
    if (!modes.includes(mode) || !colors.includes(color)) return;
    if (!validColor(input.value)) {
      input.setCustomValidity('Enter a hex color such as #2563eb.');
      input.setAttribute('aria-invalid', 'true');
      return;
    }
    input.setCustomValidity('');
    input.removeAttribute('aria-invalid');
    const value = input.value.toLowerCase();
    custom[mode][color] = value;
    fields().forEach(other => {
      if (other === input || other.dataset.customThemeMode !== mode || other.dataset.customThemeColor !== color) return;
      other.value = value;
      other.setCustomValidity('');
      other.removeAttribute('aria-invalid');
    });
    saveCustom();
    apply();
  });
  document.addEventListener('click', event => {
    const button = event.target.closest('button[data-theme-choice]');
    if (button) {
      choice = button.dataset.themeChoice;
      try { localStorage.setItem(key, choice); } catch {}
      apply();
    } else if (event.target.closest('[data-custom-theme-reset]')) {
      custom.light = {};
      custom.dark = {};
      saveCustom();
      apply();
      syncFields();
    }
  });
})();
