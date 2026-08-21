import "./styles.css";

type WorkspaceStatus = "Local workspace ready";

const workspaceStatus: WorkspaceStatus = "Local workspace ready";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) {
  throw new Error("Resonance shell mount point is missing.");
}

app.innerHTML = `
  <main class="shell" aria-labelledby="app-title">
    <aside class="navigation" aria-label="Runtime navigation">
      <p class="brand">Resonance</p>
      <nav>
        <a aria-current="page" href="#workspace">Workspace</a>
      </nav>
    </aside>
    <section class="workspace" id="workspace">
      <p class="eyebrow">Development bootstrap</p>
      <h1 id="app-title">Your local workspace is ready.</h1>
      <p>
        Resonance is running its minimal runtime shell. Content packages and
        collaboration are intentionally not loaded yet.
      </p>
      <p class="status" role="status"><span></span>${workspaceStatus}</p>
    </section>
  </main>
`;
