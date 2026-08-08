---
name: code-structural-view
description: Use when a user wants to understand the internal structure of a code component through an architecture view — its functions, types, calling hierarchy, state lifecycle, inputs, and outputs — and finds the current view unhelpful. Also use when asked to create or refactor a LikeC4 "code-level" view that reveals internal component design without showing guard/validation noise.
---

# Code Structural View Skill

Transform opaque or misleading code-level architecture views into structural decompositions
that reveal how a component is actually built. The output is a LikeC4 view that a developer
can trust as their primary map over the code.

## When to Apply

Trigger this skill when the user says any of:

- "This view isn't working for me"
- "I want to understand the internals of this component"
- "I want to see types and behavior"
- "I want to see what functions are at play and the calling patterns"
- "What are the inputs and outputs of this block of code?"
- "I want internal structure — state and behavior"
- "This should be my main view over the code"

Or when you detect that an existing code-level view is:

- A linear pipeline that doesn't match the actual call graph
- Missing key functions or types that exist in the source
- Showing implementation internals (guards, assertions) at the expense of structural understanding
- Omitting input types, output types, or internal state types

## Core Principle

**The view must be structural, not procedural.** A developer should be able to look at this
view and answer:

1. What functions exist in this component?
2. Which function calls which?
3. What types flow in, out, and through?
4. How does mutable state become the final output?
5. What external collaborators does this component depend on?

If the view can't answer those five questions, it's not done.

## Method

### Step 1: Read the source code thoroughly

Read the primary source file(s) for the component. Map every exported and internal
function, every type/interface, and every external module import. Don't skim — the goal
is to understand the real structure, not the documented structure.

### Step 2: Compare against the existing model

Read the model (via `read_model` or the `.c4`/`.likec4` files). For every function and
type in the source, ask:

- Is it in the model? If not, it needs to be added.
- Is its description accurate? If it describes internals you can't verify, it's probably
  outdated or speculative.
- Are the relationships correct? If the model shows a linear pipeline but the code has a
  tree, fix it.

### Step 3: Add missing model elements

For each function and type found in source but missing from the model, add a code element.
Use the parent scope that already exists (e.g., the package or module scope).

Use the repository model's declared `code` element kind for both functions and types. Do not invent `function` or `type` kinds unless the model's `specification` block explicitly declares them.

**Function elements:**
```likec4
functionName = code "functionName()" {
  description '''''
  One-line summary of what this function does at the structural level.
  Avoid describing internal control flow or guard logic.
  '''''
}
```

**Type elements:**
```likec4
typeName = code "TypeName" {
  description '''''
  What shape this type represents and where it sits in the lifecycle.
  '''''
}
```

**Element kinds to use:**

| Source concept | LikeC4 kind | Title example |
|---|---|---|
| Function/class | `code` | `createHost()`, `addRegistration()` |
| Interface/type alias | `code` | `MutableRegistry`, `HostContext` |
| Type guard / narrow assertion | `code` | `isPackageRegistration()` |
| Pure assertion/validation helper | `code` (but exclude from view) | `assertPath()` |

### Step 4: Wire relationships from the call graph

Trace the actual call graph from the source. Never invent a pipeline. Relationships should
reflect what the code does:

- **Factory/entry-point function** calls its helper functions: `-> calls`
- **Type guard used before processing**: `-> validates with`
- **Helper mutates internal state**: `-> mutates`
- **Internal state freezes into output**: `-> freezes into`
- **Function consumes input type**: `-> consumes`
- **Function produces output type**: `-> produces`

Use relationship kinds that make the role clear. Avoid generic `->` without a kind when
the relationship has a distinct purpose.

### Step 5: Identify state lifecycle

If the component has a mutable accumulator that gets frozen/sealed into an immutable
output, model that explicitly:

```likec4
mutableRegistryType -> hostRegistryType "freezes into"
```

This is one of the most valuable pieces of structural information — it reveals the
component's internal state machine at a glance.

### Step 6: Redesign the view

Now create the view. Follow these rules:

**Include:**
- The entry-point function (center or top)
- Internal functions it delegates to
- Types that flow in (parameters, context)
- Types that flow out (return types, registries, manifests)
- Internal mutable state types
- The state-freeze relationship
- External collaborators (imported functions from other modules)

**Exclude:**
- Pure assertion/validation helpers (guards are noise at this level)
- Error types that don't flow through the main path
- Implementation details inside functions (what a function does internally)
- Any element that you couldn't verify in the source

**Layout guidance:**
- Entry point at center or top-left
- Internal delegates fanned out from entry point
- State types with lifecycle arrows
- Output types on the right or bottom
- Input types on the left or top

**Style:**
```likec4
include *
// or be explicit about what to include/exclude
```

**Description on the view:**
Add a `description` that explains what the view shows and, importantly, what it
deliberately excludes. This sets expectations and builds trust.

### Step 7: Validate

Run `validate_architecture`. Fix any parse errors, duplicate IDs, or broken references.
Run `read_view` on the view to confirm it renders correctly and shows the right elements.

## Anti-Patterns to Avoid

1. **Linear pipeline when the code is a tree.** If `createHost` calls three helper
   functions, don't show them in a sequence. Show them as branches.

2. **Describing internals you can't verify.** Don't say "validates path format" unless
   you've read the validation code and can confirm it. Stick to structural descriptions.

3. **Including every function.** Guards, assertions, and trivial getters don't belong in
   the structural view. The question is: does this function help someone understand how
   the component is put together?

4. **Omitting types.** A structural view without types is just a call graph. The types
   are what make it an architecture view.

5. **Generic relationship kinds.** `-> calls` is fine for generic delegation, but
   `-> validates with`, `-> mutates`, `-> freezes into` carry semantic weight. Use them.

## Quick-Reference Checklist

Before declaring the view done, verify:

- [ ] Every function in the source is either in the model or deliberately excluded
- [ ] Every type (input, output, internal state) is in the model
- [ ] Relationships trace the actual call graph from the source
- [ ] State lifecycle is shown (mutable → immutable)
- [ ] External collaborators are shown
- [ ] Guards and assertions are excluded from the view
- [ ] View description says what's included and excluded
- [ ] `validate_architecture` passes
- [ ] `read_view` renders the expected elements
