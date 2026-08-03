# theview

A local cockpit for seeing the shape of the application you are building.

The first slice turns the Markdown files in any repository into a readable browser workspace:

- a project homepage;
- a navigable tree of Markdown documents;
- a document pane with safe Markdown rendering;
- a global development CLI that can be run from another repository.

## Try it

Install this checkout into a user-local bin directory:

```sh
./scripts/install-local.sh
source ~/.zshrc
```

Then run theview from any repository:

```sh
cd /path/to/another/repository
theview
```

Open the URL printed by the command. The server reads Markdown from the directory where the command was run.

## Develop theview

Install dependencies and run the tests with Bun:

```sh
bun install
bun test
```

The local Bun script delegates to the installed CLI:

```sh
bun run theview
```
