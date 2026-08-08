function createElement(documentRoot, name, attributes = {}) {
  const node = documentRoot.createElement(name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  return node;
}

export default function createCollapsibleSection({
  documentRoot = document,
  id,
  label,
  collapsed = false,
  sectionClass = 'resonance-section',
  toggleClass = `${sectionClass}-toggle`,
  itemsClass = `${sectionClass}-items`,
  indicatorClass = `${sectionClass}-indicator`,
  itemsId = `${sectionClass}-${id}-items`,
} = {}) {
  if (!id || !label) throw new Error('A collapsible section requires an id and label.');
  const section = createElement(documentRoot, 'section', { class: `${sectionClass} resonance-collapsible-section`, [`data-${sectionClass}`]: id });
  const toggle = createElement(documentRoot, 'button', { type: 'button', class: `${toggleClass} resonance-collapsible-toggle`, 'aria-controls': itemsId });
  const title = createElement(documentRoot, 'span', { class: `${sectionClass}-label` });
  const indicator = createElement(documentRoot, 'span', { class: `${indicatorClass} resonance-collapsible-indicator`, 'aria-hidden': 'true' });
  const items = createElement(documentRoot, 'div', { id: itemsId, class: `${itemsClass} resonance-collapsible-items` });
  toggle.append(title, indicator);
  section.append(toggle, items);
  let isCollapsed = false;

  function setCollapsed(nextCollapsed) {
    isCollapsed = Boolean(nextCollapsed);
    title.textContent = label;
    indicator.textContent = isCollapsed ? '▸' : '▾';
    toggle.setAttribute('aria-expanded', String(!isCollapsed));
    items.hidden = isCollapsed;
  }
  toggle.addEventListener('click', () => setCollapsed(!isCollapsed));
  setCollapsed(collapsed);

  return { element: section, toggle, items, setCollapsed, get collapsed() { return isCollapsed; } };
}
