## Rules for agents
- I learn by reading. Build in small steps, explain each change before and after.
- Before writing, give a short plan. After writing, explain the key Rust concepts used.
- No crates for search/NLP/ML (no tantivy, no embedding libs). Everything from scratch.
- Keep code simple and commented where it's non-obvious.

## Git workflow
- **Every feature lives on its own branch**, never directly on `main`. A feature = one card in `Tasks.md` or a coherent group of them (e.g. "Cargo workspace", "BM25 scorer").
  - Before starting, branch from an up-to-date `main`: `git switch -c <type>/<short-slug>`.
  - Types: `feat/` (new capability), `fix/`, `refactor/`, `chore/` (config, tooling, setup), `docs/`.
- **Commit after every small step** that builds (`cargo check` passes), so each diff is small enough to read and review.
  - Message: `<type>(<scope>): <what, imperative>`, e.g. `feat(search): add BM25 term-frequency saturation`. Add a body explaining *why* when it isn't obvious.
  - Stage files explicitly (`git add <paths>`), never `git add -A`. Don't commit `target/` or unrelated user changes.
- **Never push** and never merge into `main`. Amirali reviews the branch, then merges and pushes himself.
- When a feature is done, say so and summarise the branch's commits (`git log --oneline main..HEAD`).

## Memory: the dev vault (`~/grind/sunlit-garden-vault`)
The vault is the single source of truth for plans, tasks and decisions, and is your memory between sessions. Plain Markdown with Obsidian `[[wikilinks]]`; edit the files directly. Docs never go in this repo.

**At the start of a session** (before the first task):
- Read `Tasks.md` and the newest note in `Sessions/`.
- When starting work on a module, read its note in `Modules/`.

**While working:**
- Move cards in `Tasks.md` (Backlog → Next → Doing → Done). Put the branch name on a card when it moves to Doing, e.g. `… #setup (branch: chore/workspace)`. It's an Obsidian Kanban board: keep the `## Lane` headings, `- [ ]` card syntax and the `%% kanban:settings %%` block intact. Done cards use `- [x]`.
- A new decision goes into `Decisions.md` first (choice · why · rejected), then into `Roadmap.md` or the module note.
- Resolved questions move from `Open Questions.md` into `Decisions.md`.
- Update `Architecture.md` when the code's shape changes (crates, data model, storage).

**At the end of each step / session:**
- Write or append to `Sessions/YYYY-MM-DD.md`: Built · Learned · Decided · Next.
- Update `Home.md` → "Current focus" (module, code state, last session, next 3 tasks) and its `updated:` date.
- Update module status in `Roadmap.md` and the module note's frontmatter when it changes.
