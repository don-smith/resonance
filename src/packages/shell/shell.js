export function createShell({ documentRoot = document, navigation, mount }) {
  const roots = new Map();
  const packages = new Map();
  let activeId = null;

  function renderNavigation(items) {
    navigation.querySelectorAll('[data-package], [data-package-section]').forEach((element) => element.remove());
    const team = items.filter((item) => item.scope !== 'member');
    const personal = items.filter((item) => item.scope === 'member');
    let index = 0;
    const renderSection = (label, section) => {
      if (!section.length) return;
      const heading = documentRoot.createElement('p');
      heading.className = 'nav-section-label'; heading.dataset.packageSection = label.toLowerCase(); heading.textContent = label;
      navigation.append(heading);
      section.forEach((item) => {
        const button = documentRoot.createElement('button');
        button.className = 'primary-nav-item'; button.type = 'button'; button.dataset.package = item.id;
        button.innerHTML = `<span class="nav-index">${String(++index).padStart(2, '0')}</span><span></span>`;
        button.lastElementChild.textContent = item.label; navigation.append(button);
      });
    };
    renderSection('Team', team); renderSection('Personal', personal);
  }

  function setActive(id) {
    navigation.querySelectorAll('[data-package]').forEach((button) => {
      const active = button.dataset.package === id;
      button.classList.toggle('active', active);
      if (active) button.setAttribute('aria-current', 'page');
      else button.removeAttribute('aria-current');
    });
    roots.forEach((root, packageId) => { root.hidden = packageId !== id; });
  }

  function showError(root, error) {
    root.hidden = false;
    root.innerHTML = `<p class="shell-error"></p>`;
    root.querySelector('.shell-error').textContent = error?.message || String(error);
  }

  async function activate(id) {
    const instance = packages.get(id);
    if (!instance) throw new Error(`Package is not installed: ${id}`);
    const previousId = activeId;
    const previous = previousId ? packages.get(previousId) : null;

    for (const [packageId, packageInstance] of packages) {
      if (packageId !== id) packageInstance.deactivate();
    }

    try {
      await instance.activate();
      activeId = id;
      setActive(id);
    } catch (error) {
      instance.deactivate();
      if (previous && previousId !== id) {
        try {
          await previous.activate();
          activeId = previousId;
          setActive(previousId);
        } catch (rollbackError) {
          activeId = null;
          setActive(null);
          showError(roots.get(previousId), rollbackError);
        }
      } else {
        activeId = null;
        setActive(null);
        showError(roots.get(id), error);
      }
      throw error;
    }
  }

  navigation.addEventListener('click', async (event) => {
    const button = event.target.closest('[data-package]');
    if (!button || !navigation.contains(button)) return;
    try {
      await activate(button.dataset.package);
    } catch (error) {
      const root = roots.get(button.dataset.package);
      if (root) showError(root, error);
    }
  });

  return {
    renderNavigation,
    registerPackage(id, instance) { packages.set(id, instance); },
    createMount(id) {
      const root = documentRoot.createElement('section');
      root.className = `package-mount package-${id}`;
      root.dataset.package = id;
      root.hidden = true;
      mount.append(root);
      roots.set(id, root);
      return root;
    },
    activate,
    setActive,
  };
}
