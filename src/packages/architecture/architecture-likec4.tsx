import React from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { LikeC4Model } from '@likec4/core/model';
import { LikeC4ModelProvider, ReactLikeC4 } from '@likec4/diagram';

type RendererOptions = {
  root: HTMLElement;
  dump: unknown;
  viewId: string;
  onNodeClick(node: { id: string; modelRef?: string; title?: string; description?: string | null; technology?: string | null; links?: Array<{ url?: string; title?: string }> }): void;
  onNavigate(viewId: string): void;
};

type ResonanceColors = {
  paper: string;
  paperDeep: string;
  ink: string;
  inkSoft: string;
  muted: string;
  lineStrong: string;
  accent: string;
  accentSoft: string;
};

function resonanceColors(): ResonanceColors {
  const dark = document.documentElement.dataset.theme === 'dark';
  const fallback: ResonanceColors = dark ? {
    paper: '#181a1c', paperDeep: '#202326', ink: '#ece8e1', inkSoft: '#c9c3ba', muted: '#aaa59d',
    lineStrong: '#5a5e61', accent: '#e18a62', accentSoft: '#493126',
  } : {
    paper: '#f3f0e9', paperDeep: '#e9e5dc', ink: '#202326', inkSoft: '#5e5b56', muted: '#73716d',
    lineStrong: '#aaa59d', accent: '#bd5f37', accentSoft: '#f0d8ca',
  };
  const computed = typeof getComputedStyle === 'function' ? getComputedStyle(document.documentElement) : null;
  const token = (name: string, value: string) => computed?.getPropertyValue(name).trim() || value;
  return {
    paper: token('--paper', fallback.paper),
    paperDeep: token('--paper-deep', fallback.paperDeep),
    ink: token('--ink', fallback.ink),
    inkSoft: token('--ink-soft', fallback.inkSoft),
    muted: token('--muted', fallback.muted),
    lineStrong: token('--line-strong', fallback.lineStrong),
    accent: token('--accent', fallback.accent),
    accentSoft: token('--accent-soft', fallback.accentSoft),
  };
}

function resonanceDiagramStyles(colors: ResonanceColors) {
  return {
    theme: {
      colors: {
        primary: {
          elements: { fill: colors.accentSoft, stroke: colors.accent, hiContrast: colors.ink, loContrast: colors.inkSoft },
          relationships: { line: colors.accent, label: colors.ink, labelBg: colors.paper },
        },
        gray: {
          elements: { fill: colors.paperDeep, stroke: colors.lineStrong, hiContrast: colors.ink, loContrast: colors.muted },
          relationships: { line: colors.lineStrong, label: colors.inkSoft, labelBg: colors.paper },
        },
      },
    },
    defaults: { color: 'primary', relationship: { color: 'gray' } },
  };
}

const resonanceMantineTheme = {
  primaryColor: 'resonance',
  primaryShade: { light: 5, dark: 5 },
  colors: {
    resonance: [
      'var(--paper)', 'var(--paper-deep)', 'var(--accent-soft)', 'var(--accent-soft)', 'var(--accent)',
      'var(--accent)', 'var(--accent)', 'var(--accent)', 'var(--ink)', 'var(--ink)',
    ],
    gray: [
      'var(--paper)', 'var(--paper-deep)', 'var(--line)', 'var(--line)', 'var(--line-strong)',
      'var(--muted)', 'var(--ink-soft)', 'var(--ink-soft)', 'var(--ink)', 'var(--ink)',
    ],
    dark: [
      'var(--paper)', 'var(--paper-deep)', 'var(--line)', 'var(--line-strong)', 'var(--muted)',
      'var(--ink-soft)', 'var(--ink)', 'var(--sidebar-active)', 'var(--sidebar)', 'var(--sidebar)',
    ],
  },
  white: 'var(--paper)',
  black: 'var(--ink)',
  fontFamily: 'var(--mono)',
  headings: { fontFamily: 'var(--display)' },
};

function themedDump(dump: unknown, colors: ResonanceColors): unknown {
  const source = dump as { project?: { styles?: { theme?: { colors?: Record<string, unknown> }; defaults?: Record<string, unknown> } } };
  const styles = source.project?.styles || {};
  const defaults = styles.defaults || {};
  const relationshipDefaults = defaults.relationship as Record<string, unknown> | undefined;
  return {
    ...source,
    project: {
      ...source.project,
      styles: {
        ...styles,
        theme: { ...styles.theme, colors: { ...styles.theme?.colors, ...resonanceDiagramStyles(colors).theme.colors } },
        defaults: { ...defaults, ...resonanceDiagramStyles(colors).defaults, relationship: { ...relationshipDefaults, ...resonanceDiagramStyles(colors).defaults.relationship } },
      },
    },
  };
}

export function createLikeC4Renderer({ root, dump, viewId, onNodeClick, onNavigate }: RendererOptions) {
  const reactRoot: Root = createRoot(root);
  const render = () => {
    const model = LikeC4Model.fromDump(themedDump(dump, resonanceColors()) as never);
    const view = model.view(viewId).$view;
    reactRoot.render(
    <LikeC4ModelProvider likec4model={model}>
      <ReactLikeC4
        viewId={view.id}
        pannable
        zoomable
        reactFlowProps={{ zoomOnScroll: true, panOnScroll: false }}
        controls={false}
        fitView
        background="dots"
        colorScheme={document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light'}
        injectFontCss={false}
        style={{
          '--colors-diagram-background': 'var(--paper)',
          '--colors-diagram-background-pattern': 'var(--line)',
          '--colors-likec4-mix-color': 'var(--paper)',
          '--likec4-app-font': 'var(--mono)',
          '--likec4-app-font-default': 'var(--mono)',
        } as React.CSSProperties}
        mantineTheme={resonanceMantineTheme}
        showNavigationButtons={false}
        enableElementDetails
        enableRelationshipDetails={true}
        nodesSelectable
        onNodeClick={(node) => onNodeClick(node)}
        onNavigateTo={(to) => onNavigate(to)}
      />
    </LikeC4ModelProvider>,
    );
  };
  render();
  const themeObserver = typeof MutationObserver === 'function' ? new MutationObserver(render) : null;
  themeObserver?.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
  return {
    unmount() { themeObserver?.disconnect(); reactRoot.unmount(); },
  };
}
