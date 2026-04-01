# Full Refactor Branch Summary

Branch: `codex/full-refactor`
Base branch: `main`
Summary date: `2026-04-02`

## Goal

This branch focused on three primary goals:

1. Reduce change ripple by separating tightly coupled TUI logic into smaller modules.
2. Improve maintainability by extracting large files into clearer submodules.
3. Add tests around core logic so refactoring and performance work can be validated safely.

An additional performance pass was completed afterward to improve responsiveness and reduce
unnecessary CPU, memory, and I/O overhead in the TUI flow.

## Commit Timeline

1. `bf40977` `refactor: extract tui app filters and storage`
2. `98bc5ce` `refactor: split tui app forms and shared types`
3. `301e6b6` `refactor: extract tui runtime helpers`
4. `aa27286` `refactor: extract tui ui helper module`
5. `92c6619` `refactor: remove unused legacy api paths`
6. `afa0abb` `perf: cache filtered bookmark views`
7. `0a17efd` `perf: cache parsed model data for tui`
8. `26255ac` `perf: reduce image feed clone pressure`
9. `39c0a7f` `perf: reuse model metadata in event loop`
10. `5ee60b9` `perf: trim model search response cloning`
11. `a799d47` `fix: clamp filtered bookmark selections`

## Major Refactoring Changes

### 1. TUI app decomposition

The previous monolithic `src/tui/app.rs` responsibilities were split into focused modules:

- `src/tui/app/filters.rs`
- `src/tui/app/forms.rs`
- `src/tui/app/storage.rs`
- `src/tui/app/types.rs`

Impact:

- Search/filter logic is separated from state mutation.
- persistence logic is isolated from runtime UI state.
- shared app-facing enums/messages are easier to reuse without dragging unrelated code.

### 2. UI and runtime extraction

Supporting logic was separated from the primary rendering/event files:

- `src/tui/runtime.rs`
- `src/tui/ui/helpers.rs`

Impact:

- Render sizing and request-key logic became testable in isolation.
- `ui.rs` lost a large amount of utility noise, making render flow easier to follow.

### 3. Legacy path removal

Unused legacy API code paths were removed:

- deleted `src/api/client.rs`
- simplified callers to use the SDK-backed client path
- removed dead legacy download management branches

Impact:

- fewer duplicate network layers
- less confusion around which client path is authoritative

## Performance Work

### 1. Cached bookmark visibility views

Visible bookmark/image-bookmark lists are now cached instead of being rebuilt repeatedly from
full collections during common UI interactions.

Impact:

- lower CPU overhead while navigating filtered bookmark views
- reduced repeated allocation/cloning during render-heavy interaction

### 2. Cached parsed model metadata

Parsed model metrics, versions, and default base-model data are cached inside `App`.

Impact:

- reduced repeated JSON-derived parsing in render paths
- faster version/file navigation and sidebar rendering

### 3. Reduced image feed clone pressure

The image feed worker now avoids unnecessary vector duplication when hydrating image items,
filtering out unsupported items, updating cache, and scheduling preloads.

Impact:

- lower transient memory usage during image feed fetch
- less clone-heavy CPU work during image load bursts

### 4. Event-loop reuse of parsed model data

The event loop was updated to reuse `App`’s parsed model metadata rather than reparsing model
versions and preview data during cover-priority and prefetch flows.

Impact:

- faster response on selection changes
- less repeated parsing when opening image model detail modals

### 5. Reduced model search response cloning

Model search worker flow now trims repeated cloning of result vectors while still preserving
behavior for UI delivery, caching, and cover job extraction.

Impact:

- lower memory peak on model search responses
- less duplicated work between cache/network result paths

## Tests Added or Strengthened

The branch added or relied on tests around:

- bookmark filter behavior
- bookmark sort behavior
- search form option builders
- bookmark and image-bookmark persistence normalization
- render request sizing and request-key generation
- UI helper behavior
- bookmark/image-bookmark visibility cache updates
- filtered bookmark selection clamping after removal

## Final Review Findings

A final branch-wide review found and fixed two concrete issues:

1. Filtered bookmark removal could leave selection state out of range because clamping ran before
   the visible cache was refreshed.
2. `custom_image_sort_to_meili` contained a no-op string replacement that added confusion without
   changing behavior.

These were fixed in commit `a799d47`.

## Verification

The final review completed the following successfully:

- `cargo test`
- `cargo check`
- `cargo build`

Additional lint review with `cargo clippy --all-targets -- -D warnings` was also run. It surfaced
many non-blocking style/design warnings across older areas of the codebase, but no additional
correctness issue was identified beyond the fixes already applied in this review pass.

## Overall Result

This branch leaves the TUI code significantly more modular, more testable, and easier to change in
small increments. The highest-value performance hot paths that were repeatedly reparsing data or
cloning large collections were also reduced, improving responsiveness and lowering unnecessary work
without changing the user-facing workflow.
