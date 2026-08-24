// GENERATED FILE. DO NOT EDIT. Source: system/foundations/theme.js
(() => {
  const storageKey = "ds-theme";
  const legacyStorageKey = "ds-catalog-theme";
  const themes = new Set(["system", "light", "dark"]);
  const root = document.documentElement;
  let selectedTheme = themes.has(root.dataset.theme) ? root.dataset.theme : "system";

  try {
    const storedTheme = localStorage.getItem(storageKey) ?? localStorage.getItem(legacyStorageKey);
    if (themes.has(storedTheme)) {
      selectedTheme = storedTheme;
      localStorage.setItem(storageKey, storedTheme);
      localStorage.removeItem(legacyStorageKey);
    }
  } catch {}
  root.dataset.theme = selectedTheme;

  function bindThemeControls() {
    const buttons = [...document.querySelectorAll("[data-theme-choice]")];
    const updateButtons = () => {
      for (const button of buttons) {
        button.setAttribute("aria-pressed", String(button.dataset.themeChoice === selectedTheme));
      }
    };

    for (const button of buttons) {
      button.addEventListener("click", () => {
        selectedTheme = button.dataset.themeChoice;
        root.dataset.theme = selectedTheme;
        try {
          localStorage.setItem(storageKey, selectedTheme);
        } catch {}
        updateButtons();
      });
    }
    updateButtons();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", bindThemeControls, { once: true });
  } else {
    bindThemeControls();
  }
})();
