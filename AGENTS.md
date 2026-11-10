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
- **When a feature is done**, push its branch and open a PR:
  - `git push -u origin <type>/<slug>`, then `gh pr create --base main --head <type>/<slug>`.
  - Title = the feature in `type(scope): what` form; body = what changed, why, how to check it, and the commit list (`git log --oneline main..HEAD`).
  - A branch built on an unmerged branch gets `--base <that-branch>` and says so in the body.
  - Put the PR link on the card in `Tasks.md` and in the session note.
- **Never** push `main`, force-push, or merge PRs. Amirali reviews and merges on GitHub.

## Skills: when to use them
Only these skills are enabled in this project. Load the skill when its trigger fits; say which one you're using.
- **`tdd`** — any logic with clear right answers: tokenizer, sentence splitter, TF-IDF/BM25, parsers, argumentation semantics. Default for `core` code.
- **`diagnose`** — a bug or regression whose cause isn't obvious after one look (wrong scores, Rune glitches, flaky behaviour).
- **`design-an-interface`** — shaping a **Rust API**: the `core` crate's public types and functions, the claim-vs-argument data model, module boundaries. Not for UI.
- **`grill-me`** — before a decision that goes into `Decisions.md` (storage format, claim vs argument, ML framework). Record the outcome in the vault.
- **`handoff`** — when a session gets long or ends mid-task; complements, doesn't replace, the session note.
- **`zoom-out`** — Amirali-invoked (`/zoom-out`) when he's lost in an area of code.
- **`opencode`** — any change to OpenCode config, agents, commands or permissions.

## UI design: Rune only
All UI design and prototyping happens **in Rune itself** — no Figma, HTML mockups, generated images or web prototypes. Read the `rune` reference before using an API.
- **Prototype variants** with Rune's **Playground** (`App::playground`: stories, variants, ⌘⇧D) and **scenes** (`AppSpec::scenes`, named states with fixture data). See `examples/playground`, `examples/hello`.
- **Tune** spacing, sizes, springs live with **`rune-knobs`** (⌘⇧K panel). See `crates/rune-knobs/README.md`.
- **Components** come from **`rune-kit`** (themable, semantic tokens only — no raw style literals). Browse them in `examples/kit-gallery`.
- **Verify your own work headlessly** and show Amirali the result:
  - `--snap out.png [--scene NAME | --playground STORY --variant N] [--theme dark|light] [--size WxH]` renders a PNG with no window.
  - `--tree out.json` dumps the element tree; `--script FILE.json` replays scripted input.
- Put renders in a temp dir, not the repo.

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
