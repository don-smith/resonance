(() => {
  const storageKey = 'resonance:theme';
  const preferences = new Set(['light', 'dark', 'system']);
  let preference = 'system';
  try {
    const stored = window.localStorage?.getItem(storageKey);
    if (preferences.has(stored)) preference = stored;
  } catch { /* browser storage is optional */ }
  let systemTheme = 'light';
  try {
    systemTheme = window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } catch { /* light is the safe fallback */ }
  document.documentElement.setAttribute('data-theme-preference', preference);
  document.documentElement.setAttribute('data-theme', preference === 'system' ? systemTheme : preference);
})();
