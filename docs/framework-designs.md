# Growing hdru with hd's effect system

Design proposal against hdru `fb04ea9` and hd-lang `07e1e5ce8`. The first target is a long-lived application that shares its UI between a browser and a terminal. The numbered sections preserve the original design. The implementation and its verification are listed at the end.

## What the effect system gives us

hd separates three decisions that matter to a UI. A requirement row specifies which providers the caller must supply. A `$.with` scope selects the providers. A `Suspend[T]` represents one execution whose providers were chosen when the cold frame was constructed. Recoverable failures travel as `Result[T, E]`. This lets a component ask for a store, a request use a test clock, and a worker cancel a load without introducing language syntax. [hd requirement model](../../hd-lang/spec/lang/11-requirements-and-suspension.md#decomposed-model) · [Construction-time binding](../../hd-lang/spec/lang/11-requirements-and-suspension.md#construction-time-requirement-binding)

| Code that runs | Row or value it receives | Who supplies it |
| --- | --- | --- |
| Component construction | `$ Ui`, currently `Owner + Parent`, plus named application services | The mount supplies the owner and parent. Synchronous builders forward the application's `$R`. |
| Derivation or binding | `$ Track` | The running computation installs a fresh collector. |
| Event handler or action submission | `$ Mutate` and the explicitly captured application services | Dispatch supplies a writer for this graph and batches the handler. |
| Loader | A tracked key computed synchronously, then a cold `Suspend[Result[T, E]]` | The loader factory binds its services when making the frame. A worker drives it once. |
| Host I/O | A package's `@host` trait | The entry adapter obtains the host provider with the manifest's consent. Ordinary library keys such as `Owner` and `Track` need no host grants. |

A saved callback can capture a counted provider, or a concrete bundle such as `$.Context[$ Clock + Api]`, and reinstall it with `$.with(services...)`. An arbitrary `$.Context[$R]` is rejected. Use row polymorphism to *forward a call* and concrete contexts to *retain named services*. Never save the active dependency collector for later. A row describes unresolved lookups, so a closure can still use authority explicitly captured in a value. [Concrete context restriction](../../hd-lang/spec/lang/11-requirements-and-suspension.md#row-parameters) · [Provider values](../../hd-lang/spec/lang/11-requirements-and-suspension.md#provider-values)

For example, a component that can look up `Catalog` can build this loader and hand it to any worker. The compiler checks the dependency at the factory call. A test binds a fake catalog at the same place.

```hd
type TitleLoader = fn!(i32) -> Result[string, string]

fn title_loader() -> TitleLoader $ Catalog:
    services := $.context(Catalog=$.use(Catalog))
    fn!(id: i32) -> Result[string, string]:
        $.with(services...):
            load_title!(id)
```

In a standalone probe, the caller built the loader under a catalog named `mount`, left that scope, and invoked it for ID 7 under a catalog named `worker`. It returned `mount 7`.

### 1. Make a mount disposable and a signal reclaimable

Give the reactive package a graph instance and the UI package a mount handle. The graph owns computations, owners and its scheduler. Each mount owns its root owner, host subtree, command queue, event handlers, ready callbacks and jobs. By default an app gets a fresh graph. Two roots can deliberately share a graph to display the same model while retaining separate surfaces and queues.

Change the owner provider to return a graph-qualified owner handle. Make the existing parent and target providers identify their mount as well as their node. Builders can then use their existing rows. A text binding captures its destination when built and can later emit to that mount under `$ Track`. Dispatch installs a writer for the right graph. A free function copies pending work out of the graph, releases the loan, then runs user code. Calling a callback inside a graph provider's `&mut self` method can otherwise cause a reentrant borrow conflict. [Current runtime](../ui/src/runtime.hd) · [hd call loans](../../hd-lang/spec/lang/13-values-and-references.md#r-ref.excl.alias.call-loan)

Each surface gets one ordered command writer. If a batch in A changes a shared signal displayed in B, B's empty-to-nonempty outbox transition wakes its writer even while B is waiting for input. A one-shot channel per idle period suffices. The writer installs the sender after checking the empty queue, emitting closes it synchronously, and the writer rechecks the queue before sleeping. Keep one persistent input reader per surface so an output wake cannot consume or lose a native event.

Keep a plain signal's value and version in its counted slot. Give it a graph identity and a monotonic source ID. Add an entry to the graph's subscription index on its first observer, and erase it when its last observer unsubscribes. A computation holds its source slots while subscribed. With zero handles and zero subscribers a row signal is reclaimable. An application that deliberately retains a signal can still `peek` it, update it, or subscribe again. Memo disposal detaches its producer and freezes the last computed value for any surviving read handle. This avoids invalidating a copied handle just because one view closed. [Current lifetime rule](design.md#the-reactive-core) · [Existing escaped-signal test](../reactive/tests/core.hd)

**Proposed API change.** `signal(initial) $ Owner` takes its graph from the owner. Standalone state uses an explicit `graph.signal(initial)`, and imperative updates enter that graph's mutation batch. This changes today's row-free constructor. Introduce the explicit graph and mount APIs first, then migrate the constructor and setup code together. A tracked read or write with a different graph reports a graph-identity error before subscribing or changing the value. Existing user-facing `get`, `peek` and `set` retain their phase distinctions. Enforce the write policy before publishing the value or marking observers.

`unmount` first marks the mount closing, rejects further delivery, invalidates jobs and drops unapplied callbacks. It disposes the root owner, runs subtree cleanups, detaches subscriptions and removes this mount's host nodes and listeners. Keep a small closing record until workers and the command writer have acknowledged shutdown, then release the remaining tables. A repeated close is harmless. A generation on the surface/event envelope keeps a queued event from an old mount from activating handler 1 in its replacement. A suspending close waits for started jobs to clean up. Preserve the existing scripted loop's normal end-of-input behavior by draining accepted jobs before closing. An explicit unmount or host disconnect requests cancellation immediately. [Current drain behavior](../ui/src/renderer.hd) · [Loop tests](../ui/tests/loop.hd)

**Acceptance.** Churn fresh row keys and assert that the graph's live source, owner, computation and handler counts return to baseline after disposal. Mount A and B, unmount A twice, then edit B. Also test a signal deliberately retained across disposal, a stale event after remount, and a queued ready callback whose node has gone away.

### 2. Separate changes to order from changes to values

First fix `for_each` without changing its API. Compute each key once and check duplicates with a hash set before emitting commands. Index the old keys by position. For each new key, retain the existing row or create one, and use a membership lookup to remove missing rows. If retained positions are increasing, skip move planning. Otherwise compute their longest increasing subsequence (LIS). Walk the target order from the end, placing new or moved fragments before the next known sibling. The LIS rows stay put. New rows can use the existing append-then-move protocol initially. For equal-length LIS choices, use a deterministic tie rule and preserve existing command fixtures where equivalent. [Current reconciler](../ui/src/collections.hd) · [Vue's keyed reconciler](https://github.com/vuejs/core/blob/main/packages/runtime-core/src/renderer.ts)

For ordinary hash costs the **planner** is linear on unchanged order, append and deletion, and `O(n log n)` for a general permutation, with `O(n)` scratch storage. Rotating `A B C D` to `B C D A` takes one retained-row move. Disposing each deleted row also linearly searches its parent's child list. Give the owner tree constant-time unlinking, for example with sibling links that preserve traversal order. Measure renderer application separately, especially terminal layout. [Owner removal](../reactive/src/graph.hd)

Then add a keyed collection for apps that edit data frequently. Store an ordered key signal plus a map from key to stable row model. A structural edit batches the map and order changes. A field edit writes the affected cell directly and leaves order untouched. Start with ordinary hd structs containing signal fields, such as a todo with a title cell and a done cell. Keep `for_each` for snapshot lists, and add a keyed view over the ordered keys for editable models. Filtering that reads a title legitimately recomputes membership when that title changes. Virtualization can be a view over ordered keys and a viewport. Offscreen row owners are disposable and application state stays in the model.

**Acceptance.** Count key evaluations, row creations, moves, disposals and text writes for unchanged data, one edit, rotation, reversal and bulk removal. In an unfiltered list, editing one title through the keyed model should run zero order reconciliations and update one title binding. Include duplicate keys and focus retention after a reorder.

### 3. Give resources refresh, cancellation, boundaries and mutations

Keep `resource` and `resource_with` as compatible entry points. Add a resource handle for callers that need `snapshot.get() $ Track` and `refresh() $ Mutate`. Its states are initial loading, ready with a value, refreshing with the previous value, and failed with an error and an optional previous value. The compatibility view maps refreshing to the existing `Loading` case. Retain a previous value on refresh of the same logical record. A key change clears the previous value by default. Search can explicitly retain old results across query keys and mark them stale. A profile for a different account should start empty. [Current resource transitions](../ui/src/resource.hd)

Give each job a generation and its own initially empty channel. The resource retains the sender. A key change, owner disposal or explicit cancellation closes that sender synchronously. Remove a queued cold job immediately. A worker taking a live job races two children with the same outcome type:

```text
race!(wait_for_close(stop_receiver), drive_load(cold_job))
    Superseded       → release the worker slot after child cleanup
    Finished(result) → publish only if the mount, owner and generation still match
```

Only the worker drives the load. The close branch returns an ordinary outcome, and `race!` cancels the losing load and waits for cleanup. Generation checks also cover completion and invalidation in the same turn. Four superseded loads can release all four slots without cancelling the worker pool. Cancellation is cooperative. A provider that aborts slowly can still hold a slot during cleanup. Put an explicit cleanup deadline on resources that need one, using the existing task policy. Keep `Clock` on that policy path. [hd race semantics](../../hd-lang/spec/lang/11-requirements-and-suspension.md#standard-combinators) · [Server SDK request cancellation](../../hd-lang/packages/server-sdk/src/server.hd) · [Task cleanup bounds](../../hd-lang/spec/std/task.md#cleanup-bound)

Queue at most one pending entry per resource, keyed by its identity. A superseding key replaces its queued cold frame in place. If a load is already running, close it and queue only the latest replacement. This bounds queue growth during a burst of keystrokes even when every worker is busy cleaning up.

Build a panel boundary over explicit resource handles. It derives the group's pending and error state, displays an initial fallback, and keeps ready content visible during refresh. The owner of the handles lives above the visual branch, so hiding the branch cannot dispose the very load needed to reveal it. Retry invokes the failed handle's refresh. Different error types map to a panel error at registration. Dependent loading starts when an upstream snapshot is ready, with disposal and generations on each stage. Read all tracked inputs when computing the key. A read after suspension would otherwise miss the original tracking scope. Recoverable render failures can use an explicit `Result`-returning builder. A panic terminates the hd instance and cannot be caught by a library boundary. [hd panic rule](../../hd-lang/spec/lang/06-control-flow.md#r-flow.panic.def) · [Solid's boundary behavior](https://docs.solidjs.com/concepts/boundaries/)

For optimistic saves, keep confirmed data and a small ordered list of pending edits per entity. Submission adds an edit under `$ Mutate`, and a memo derives the visible value by applying the edits to the confirmed value. Serialize saves for the same entity in submission order, allow different entities to save concurrently, and record a distinct mutation ID per save. On success, merge the authoritative returned value or revision, remove that edit and reapply later edits. On failure, remove only the failed edit and expose its error for retry. The entity or application owns a submitted save if it should survive leaving the editor. Cancelling its local wait cannot undo a server write. Use a mutation ID accepted by the server when retrying an ambiguously completed write.

**Acceptance.** Start A, supersede it with B while A is awaiting, observe A's cleanup, and verify B lands. Repeat with every worker occupied, a closing mount, an uncancellable host completion and a retry after failure. Submit two edits to one record, fail the first, and verify the second remains visible. Test with controlled channels and scripted providers so the ordering is deterministic.

### 4. Make a labeled, validated field portable

Introduce typed listener helpers for text edits, toggles, selection, key chords and submit. At the host boundary use a tagged event payload with a mount generation and handler ID. At registration store a small adapter that checks the expected tag and invokes a concrete callback such as `FnMut(bool) -> void $ Mutate`. An incompatible event produces a diagnostic and no mutation. Keep the existing string listener as an adapter for callers and custom DOM events. Version the host schema and regenerate both providers' glue and fixtures together. Existing compiled hosts need their matching protocol version. [Current event ABI](../ui/src/protocol.hd) · [Controlled input and echo suppression](../ui/src/inputs.hd)

Make a field handle hold a stable identity and typed value. A field builder requires a visible label or an explicit accessible name. Give each control a declared disabled state and an optional reactive error description. Keep validation a plain function returning `Result`, expose its result through a memo, and let the submit handler mark invalid fields and request focus on the first one. A select exposes stable application option keys and their localized labels. The DOM renderer associates real labels, controls, descriptions and native select options. The TUI renders the label and error beside the control, supports keyboard selection, and moves its focus cursor. Browser screen-reader behavior needs a browser test. [W3C form labels](https://www.w3.org/WAI/tutorials/forms/labels/)

Field decoration still requires `$ Owner + Target`. A retained submit handler captures the field handle and requests focus under `$ Mutate`. The renderer applies the request after the target exists, and ignores a removed or disabled target. Keyboard default prevention stays declarative at registration, as in `take_keys`, since the Wasm handler runs after native dispatch. Suppress submit during IME composition. Preserve the existing rule that a user edit does not echo its value back and move the caret. [Existing keyboard contract](layering.md#conventions-for-a-builder)

**Acceptance.** Exercise the same form using a recorder, a real browser and the TUI. Verify accessible name and error association in the browser, keyboard-only selection and focus in both renderers, IME Enter, and disposal before a queued focus request.

### 6. Make the toolchain reproducible

Record one tested hd-lang source revision for the CLI, stdlib, host TypeScript, glue generator and browser compiler. Record the Node, pnpm and Rust versions used to build it. Resolve the compiler binary and imports from that checkout in both local and CI entry points. Key cached artifacts by those inputs plus the host schema. The current playground reuses an existing compiler Wasm by file presence and the README names different test and browser revisions. If the playground still needs a patch, pin its exact commit and run the entire matrix on that commit before declaring support. [Current setup](../README.md#try-it) · [Playground build](../web/playground/build.mjs)

CI should build from an empty artifact cache, run the hd workspace suite and Node provider/layering/editor tests, run a compiled terminal example, and build and exercise a browser example and the playground. A separate upstream compatibility job can report compiler drift without changing the release pin. The stored suspension spelling illustrates why this matters. The older documented compiler required writable suspension types in some positions, while the inspected mainline spec rejects a leading `mut` on a type. Choose the pin by passing the matrix. [Current grammar](../../hd-lang/spec/lang/02-grammar.md#r-grammar.type.no-mut)

## Delivery order and evidence

The stack implements the pinned toolchain, graph and mount lifetimes, keyed planner and collections, cancellable queries and optimistic actions, and portable validated fields. The [record editor](../examples/editor/src/lib.hd) exercises them together on the DOM and terminal.

| Workload | Checked behavior |
| --- | --- |
| Page through 1,000 records | 12 row owners on the first page. An unfiltered title edit sends one text command and skips order reconciliation. A four-row rotation sends one move. [Collection tests](../ui/tests/for_each.hd) |
| Replace every row 25 times, then close | Live source, computation, owner and handler counts stay at baseline after each replacement and reach zero on close. An idle second mount receives a shared write. [Mount tests](../ui/tests/mounts.hd) |
| Supersede four blocked loads | Four active workers clean up and become available for replacements. Explicit close waits for cleanup. Retrying a failed save reuses its mutation ID. [Async tests](../ui/tests/async.hd) |
| Search, edit and save | Both renderers run the same compiled component. Chromium checks accessible names, validation focus, retained focus across reorder, and retry after closing a failed editor. The terminal checks keyboard-only search, selection and editing. [Browser smoke test](../scripts/browser-smoke.mjs) · [Terminal integration test](../ui-tui/providers/tui.test.mjs) |

After building the examples and playground, `node scripts/browser-smoke.mjs` prints elapsed time from the browser save click to the DOM update. With the pinned CLI on PATH, `node --test --test-name-pattern="compiled editor" ui-tui/providers/tui.test.mjs` measures the keypress to the updated terminal frame over pipes. The measurements include different host paths and vary by machine. CI checks the resulting state and command counts.
