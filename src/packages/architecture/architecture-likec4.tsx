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

export function createLikeC4Renderer({ root, dump, viewId, onNodeClick, onNavigate }: RendererOptions) {
  const model = LikeC4Model.fromDump(dump as never);
  const reactRoot: Root = createRoot(root);
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
        mantineTheme={{ white: 'var(--paper, #f7f8f6)' }}
        showNavigationButtons={false}
        enableElementDetails
        enableRelationshipDetails={true}
        nodesSelectable
        onNodeClick={(node) => onNodeClick(node)}
        onNavigateTo={(to) => onNavigate(to)}
      />
    </LikeC4ModelProvider>,
  );
  return {
    unmount() { reactRoot.unmount(); },
  };
}
