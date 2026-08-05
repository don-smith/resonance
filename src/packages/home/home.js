export default function createHomePackage({ fetchFn = fetch } = {}) {
  let root;
  let contentElement;

  function showError(error) {
    contentElement.innerHTML = '<p class="home-error"></p>';
    contentElement.querySelector('.home-error').textContent = error?.message || String(error);
  }

  return {
    mount(mountRoot) {
      root = mountRoot;
      root.innerHTML = '<article class="home-content"><div class="home-body"><p class="home-loading">Loading Home…</p></div></article>';
      contentElement = root.querySelector('.home-body');
    },
    async activate() {
      root.hidden = false;
      contentElement.innerHTML = '<p class="home-loading">Loading Home…</p>';
      const response = await fetchFn('/api/home');
      if (!response.ok) {
        const error = new Error('Home source could not be loaded.');
        showError(error);
        throw error;
      }
      const home = await response.json();
      contentElement.innerHTML = home.html;
    },
    deactivate() {
      root.hidden = true;
    },
  };
}
