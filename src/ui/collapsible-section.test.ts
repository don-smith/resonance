import test from 'node:test';
import assert from 'node:assert/strict';
import { parseHTML } from 'linkedom';
import createCollapsibleSection from './collapsible-section.js';

test('creates an accessible collapsible section with a stable item slot', () => {
  const { document } = parseHTML('<!doctype html><body></body>');
  const section = createCollapsibleSection({ documentRoot: document, id: 'status', label: 'In progress', collapsed: true });
  const item = document.createElement('button'); item.textContent = 'Decision'; section.items.append(item); document.body.append(section.element);

  assert.equal(section.toggle.textContent, 'In progress▸');
  assert.equal(section.toggle.getAttribute('aria-expanded'), 'false');
  assert.equal(section.items.hidden, true);
  assert.equal(section.items.id, 'resonance-section-status-items');
  assert.equal(section.items.textContent, 'Decision');

  section.toggle.click();
  assert.equal(section.items.hidden, false);
  assert.equal(section.toggle.getAttribute('aria-expanded'), 'true');
  assert.equal(section.toggle.textContent, 'In progress▾');
  section.setCollapsed(true);
  assert.equal(section.items.hidden, true);
});
