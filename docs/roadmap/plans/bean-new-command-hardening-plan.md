# Bean `new` Command Hardening Implementation Plan

## Goal

Implement `bean new html` as a user-friendly, safe, real-world project scaffolding command.

The current implementation has several structural issues:

- It validates the scaffold target with `check_if_valid_path`, which requires the path to already exist. Project creation must be able to create missing target directories.
- It treats the provided path as a parent directory, then appends `project_name` as another path segment.
- It asks for a project name, but also uses that value as a filesystem component.
- It writes both `#project_name` and `#name`, even though config loading treats both as project-name keys.
- It omits `src/`, `src/#page.bst`, `lib/`, and output manifests.
- It has a broken `full_path.join("../..")` directory creation.
- It does not give enough explicit CLI feedback for destructive or ambiguous operations.

This plan replaces the existing ad-hoc scaffold logic with a small, testable module that resolves targets, prompts clearly, checks conflicts before writing, supports a narrow `--force` escape hatch, and generates a complete default HTML project.

## Agreed behavior

### Command shape

Supported command:

```bash
bean new html [path] [--force]
```

`--force` is scaffold-specific and should be parsed only by `new html`.

No `--yes`, preset selection, or non-interactive mode will be implemented now. These are deferred and should be recorded in the roadmap.

### Path semantics

`[path]` always means the target path the user is pointing at.

Accepted path forms:

| Input | Behavior |
|---|---|
| omitted | use current directory flow |
| `.` | current directory |
| `./site` | relative to current directory |
| `/absolute/path/site` | absolute target |
| `~/site` | expand to home directory |
| paths containing `..` | allow after normal resolution/canonicalization where safe |

### Missing path

If no path is provided, show the current directory and ask whether to scaffold there.

```text
No project path specified. Current directory: /path/to/current
Create the new HTML project in this directory? [y/N]:
```

If confirmed, the current directory is the target. No extra nested folder is created.

### Existing path with no new directories

If the provided path exists, or resolves to an existing directory, ask explicitly whether to scaffold inside it or create a child folder.

```text
Target directory already exists:
/path/to/target

What do you want to do?
  1. Create the project inside this directory
  2. Create a new child folder for the project inside this directory
  3. Cancel

Choose [1/2/3]:
```

If option 2 is selected:

```text
Project folder name:
```

The target becomes `/path/to/target/<project-folder-name>`.

### Path with missing directories

If the path contains directories that do not exist, clearly ask before creating them.

```text
The project target contains directories that do not exist:
/path/to/new/site

Create the missing directories and scaffold the project there? [y/N]:
```

### Project name prompt

Always ask for the project display/config name after the final project directory is known.

```text
Project name (press Enter to use project directory name):
```

If skipped, use the final project directory basename.

Rules:

- Trim surrounding whitespace.
- Allow spaces, dots, hyphens, Unicode, and other normal display-name characters.
- Escape `\` and `"` when writing into `#name = "..."`.
- Reject empty only when there is no valid directory basename fallback.

### Existing non-empty directories

`bean new html` may scaffold into an existing non-empty directory, but only after a clear warning and confirmation.

It must not overwrite existing scaffold-owned files unless `--force` is used.

Conflict files without `--force`:

```text
#config.bst
src/#page.bst
dev/.beanstalk_manifest
release/.beanstalk_manifest
```

Existing directories are not conflicts by themselves:

```text
src/
lib/
dev/
release/
```

Reason: `create_dir_all` is non-destructive. Files are the overwrite risk.

### `--force`

`--force` allows replacing scaffold-owned files only:

```text
#config.bst
src/#page.bst
dev/.beanstalk_manifest
release/.beanstalk_manifest
```

It must not:

- delete unrelated files
- clear directories
- overwrite arbitrary files under `src/`
- overwrite or replace an existing `.gitignore`

Even with `--force`, show a second warning:

```text
WARNING: --force will overwrite existing Beanstalk scaffold files in:
/path/to/project

Files that may be replaced:
  #config.bst
  src/#page.bst
  dev/.beanstalk_manifest
  release/.beanstalk_manifest

Continue? [y/N]:
```

### `.gitignore`

Ask whether to add a `.gitignore`. Default is yes.

If no `.gitignore` exists:

```text
Add a .gitignore with Beanstalk defaults? [Y/n]:
```

Generated `.gitignore`:

```gitignore
# Beanstalk development output
/dev

# OS/editor noise
.DS_Store
.vscode/
.idea/
```

Do not ignore `/release` by default.

If `.gitignore` already exists, ask whether to append missing Beanstalk defaults.

```text
.gitignore already exists.
Add missing Beanstalk defaults to it? [Y/n]:
```

Append only this block when missing:

```gitignore

# Beanstalk
/dev
```

Do not overwrite `.gitignore`, even with `--force`.

### Generated project shape

Default scaffold:

```text
project-root/
├── #config.bst
├── src/
│   └── #page.bst
├── lib/
├── dev/
│   └── .beanstalk_manifest
├── release/
│   └── .beanstalk_manifest
└── .gitignore        # optional, default yes
```

### Generated `#config.bst`

```bst
# project = "html"
# entry_root = "src"
# dev_folder = "dev"
# output_folder = "release"
# page_url_style = "trailing_slash"
# redirect_index_html = true
# name = "<project name>"
# version = "0.1.0"
# author = ""
# license = "MIT"
# html_lang = "en"
```

Do not include:

```bst
# origin = ...
# html_title_postfix = ...
```

Those are project-specific.

Do not write both `#project_name` and `#name`.

### Generated `src/#page.bst`

Use this exact starter page, with a trailing newline:

```bst
# page_title = "Welcome"
# page_head = [$html:
    <style>
        [$css:
            body {
                background-color: light-dark(hsl(125, 67%, 97%), hsl(203, 68%, 8%));
                padding: var(--bst-spacing--small);
            }
        ]
    </style>
]

[$markdown:
    # Welcome to Beanstalk

    Here's the @https://nyejames.github.io/beanstalk/docs/ (documentation).

    Use **bean dev** to start the development server and see your changes to this page in real time!
]
```

### Output manifests

Create empty HTML build manifests in both `dev/` and `release/`:

```text
# beanstalk-manifest v2
# builder: html
# managed_extensions: .html,.js,.wasm
```

Use the current manifest filename:

```text
.beanstalk_manifest
```

### Final CLI summary

After successful creation, print a summary:

```text
Created Beanstalk HTML project:
  Project path: /path/to/site
  Project name: site

Created:
  #config.bst
  src/#page.bst
  lib/
  dev/.beanstalk_manifest
  release/.beanstalk_manifest
  .gitignore

Next:
  cd /path/to/site
  bean check .
  bean dev .
```

If files were skipped, appended, or replaced, show `Updated:`, `Skipped:`, and/or `Replaced:` sections.

### Failure behavior

Do not implement full transactional rollback.

Instead:

1. Perform preflight conflict checks before writing anything.
2. Create directories with `create_dir_all`.
3. Write scaffold-owned files only after all conflict checks pass.
4. On write failure, print a precise message and note that creation may be partial.

Example:

```text
Project creation failed while writing src/#page.bst: permission denied.
Some scaffold directories may already have been created. No existing files were overwritten.
```

### Post-create validation

Do not run `bean check` automatically.

The command creates files. It should not also run compilation. Tests should verify the generated scaffold is valid enough to build/check.

---

## Current repo anchors

Primary implementation areas:

```text
src/projects/cli.rs
src/projects/html_project/new_html_project.rs
src/projects/html_project/mod.rs
```

Current `new_html_project.rs` should be replaced with a module folder:

```text
src/projects/html_project/new_html_project/
├── mod.rs
├── options.rs
├── prompt.rs
├── target.rs
├── scaffold.rs
└── templates.rs
```

Current docs/roadmap areas:

```text
docs/src/docs/getting-started/#page.bst
docs/src/docs/project-structure/#page.bst
docs/roadmap/roadmap.md
docs/roadmap/plans/bean-new-command-hardening-plan.md
```

Relevant existing code:

- `src/projects/cli.rs` parses commands and currently dispatches `Command::NewHTMLProject(String)`.
- `src/projects/html_project/new_html_project.rs` currently owns all scaffold creation logic and should be split.
- `src/build_system/output_cleanup.rs` owns `.beanstalk_manifest` constants and behavior, but the manifest constants are currently private.
- `docs/src/docs/getting-started/#page.bst` currently documents older `new` behavior.
- `docs/src/docs/project-structure/#page.bst` currently documents the project structure and scaffold tree.
- `docs/roadmap/roadmap.md` is the main roadmap list and should link the new plan/follow-ups.

---

## Phase 1 — CLI parsing and scaffold options

### Summary

First, make the command shape explicit without changing file writes yet. The CLI should understand `--force` only for `bean new html`, and the scaffold function should receive a typed options object instead of loose `String` and `Vec<Flag>` inputs.

This phase isolates command parsing behavior and prevents scaffold-only concerns from leaking into shared build flags.

### Implementation steps

1. Update `src/projects/cli.rs`.

2. Replace:

```rust
enum Command {
    NewHTMLProject(String),
    ...
}
```

With something like:

```rust
enum Command {
    NewHTMLProject(NewHtmlProjectOptions),
    ...
}
```

3. Add an options type in the new module:

```rust
pub struct NewHtmlProjectOptions {
    pub raw_path: Option<String>,
    pub force: bool,
}
```

4. Update `parse_new_command`:

Supported:

```bash
bean new html
bean new html site
bean new html site --force
bean new html --force site
```

Rejected:

```bash
bean new html a b
bean new html --yes
bean new html --template blog
bean build --force
bean dev --force
```

5. Keep `--force` out of the shared `Flag` enum.

6. Update help output:

```text
new html [path] [--force] - Creates an HTML project scaffold
```

7. Add/adjust CLI parser tests in `src/projects/tests/cli_tests.rs`:

- parses `new html`
- parses `new html site`
- parses `new html site --force`
- parses `new html --force site`
- rejects multiple paths
- rejects unknown `new html` flags
- confirms `build --force` is still rejected

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Parse hardened bean new html options
```

Review checklist:

- `--force` is scoped only to `new html`.
- No scaffold-only flags added to shared `Flag`.
- CLI parsing tests cover accepted and rejected shapes.
- Parser logic remains readable and does not duplicate flag parsing unnecessarily.
- `cargo test` targeted to CLI tests can be run during development, but final project validation remains `just validate`.

---

## Phase 2 — Split `new_html_project` into a testable module

### Summary

The scaffold implementation needs path resolution, prompt handling, conflict detection, template rendering, file writing, and summary reporting. Keeping this in one file will quickly become messy.

Split it into a folder module with a clear orchestration `mod.rs`.

### Implementation steps

1. Replace:

```text
src/projects/html_project/new_html_project.rs
```

With:

```text
src/projects/html_project/new_html_project/
├── mod.rs
├── options.rs
├── prompt.rs
├── target.rs
├── scaffold.rs
└── templates.rs
```

2. Update `src/projects/html_project/mod.rs` if needed so `pub(crate) mod new_html_project;` still resolves.

3. In `mod.rs`, expose the public entrypoint:

```rust
pub fn create_html_project_template(
    options: NewHtmlProjectOptions,
    prompt: &mut impl Prompt,
) -> Result<CreateProjectReport, String>
```

Or, if keeping production call simpler:

```rust
pub fn create_html_project_template(options: NewHtmlProjectOptions) -> Result<(), String>
```

With an internal terminal prompt adapter. For testability, prefer injecting prompt behavior into an internal function:

```rust
pub fn create_html_project_template(options: NewHtmlProjectOptions) -> Result<(), String> {
    let mut prompt = TerminalPrompt::new();
    create_html_project_template_with_prompt(options, &mut prompt)
}
```

4. Add a prompt abstraction:

```rust
pub trait Prompt {
    fn ask(&mut self, message: &str) -> Result<String, String>;
    fn confirm(&mut self, message: &str, default: bool) -> Result<bool, String>;
}
```

5. Implement:

```rust
pub struct TerminalPrompt;
```

Using stdin/stdout and `saying::say!()`/`print!()` where appropriate.

6. Add test-only scripted prompt:

```rust
struct ScriptedPrompt {
    responses: VecDeque<String>,
    messages: Vec<String>,
}
```

7. Add a report type:

```rust
pub struct CreateProjectReport {
    pub project_path: PathBuf,
    pub project_name: String,
    pub created: Vec<PathBuf>,
    pub updated: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub replaced: Vec<PathBuf>,
}
```

8. Keep `Result<_, String>` for now. Do not pull `CompilerError` into scaffolding.

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Split HTML project scaffolding into focused modules
```

Review checklist:

- `mod.rs` shows the scaffold flow clearly.
- No user-input `unwrap()`/`panic!`.
- Each file has one responsibility.
- Prompt handling is testable without stdin.
- Existing behavior may still be incomplete, but the module shape is stable.
- Run focused unit tests created so far.

---

## Phase 3 — Target path resolution and interactive project placement

### Summary

Implement the agreed target-resolution behavior before writing files. This phase should decide where the project goes and what it is called, but not yet perform all scaffold writes.

### Implementation steps

1. Implement target resolution in `target.rs`.

Suggested types:

```rust
pub struct ResolvedProjectTarget {
    pub project_dir: PathBuf,
    pub project_name: String,
    pub missing_directories: Vec<PathBuf>,
    pub target_existed: bool,
    pub target_was_non_empty: bool,
}
```

2. Support:

- omitted path
- `.`
- relative paths
- absolute paths
- `~` home expansion
- `..` path segments after safe normalization

3. For omitted path:

- Resolve `env::current_dir()`.
- Show current directory.
- Ask whether to use it.

4. For existing directories:

- If the target exists, show the three-option prompt:
  1. create inside this directory
  2. create child folder inside this directory
  3. cancel

5. For missing directories:

- Determine nearest existing ancestor.
- Ask before creating missing directories.

6. Detect if final project directory already exists and is non-empty.

7. Ask for project name after the final directory is known:

```text
Project name (press Enter to use project directory name):
```

8. Implement string escaping for generated Beanstalk config string literals:

```rust
fn escape_beanstalk_string_literal(value: &str) -> String
```

Minimum escaping:

- `\` → `\\`
- `"` → `\"`

9. Add tests:

- omitted path uses current directory after confirmation
- `.` resolves to current directory
- relative child path resolves under current directory
- absolute path is accepted
- `~/site` expands to home directory
- existing directory option 1 uses directory directly
- existing directory option 2 creates child folder
- option 3 cancels
- skipped project name uses directory basename
- explicit project name overrides basename
- quote/backslash escaping works

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Resolve bean new project targets interactively
```

Review checklist:

- No filesystem writes beyond what this phase intentionally tests.
- Prompt messages are clear and explicit.
- Target path logic is not mixed with file template rendering.
- Test names describe behavior, not implementation detail.
- No `check_if_valid_path` usage remains in scaffold target resolution.

---

## Phase 4 — Conflict detection, `--force`, and preflight safety

### Summary

Before writing files, compute exactly what the scaffold will create, append, replace, or skip. This phase prevents accidental overwrites and defines `--force` behavior.

### Implementation steps

1. Define scaffold-owned files:

```rust
const CONFIG_FILE: &str = "#config.bst";
const PAGE_FILE: &str = "src/#page.bst";
const DEV_MANIFEST: &str = "dev/.beanstalk_manifest";
const RELEASE_MANIFEST: &str = "release/.beanstalk_manifest";
```

2. Define scaffold directories:

```rust
src/
lib/
dev/
release/
```

3. Existing directories are not conflicts.

4. Existing scaffold-owned files are conflicts unless `--force` is set.

5. `.gitignore` is handled separately and is never overwritten.

6. If conflicts exist and `--force` is false, fail with a clear message:

```text
Cannot create project because scaffold-owned files already exist:
  #config.bst
  src/#page.bst

Run with --force to replace scaffold-owned files.
```

7. If `--force` is true and conflicts exist, show the second warning and require confirmation.

8. If an existing directory is non-empty, show a strong warning and require confirmation even when there are no file conflicts.

9. Add tests:

- existing `src/`, `lib/`, `dev/`, `release/` are allowed
- existing `#config.bst` fails without force
- existing `src/#page.bst` fails without force
- `--force` asks for second confirmation
- `--force` replaces only scaffold-owned files
- `--force` does not overwrite `.gitignore`
- non-empty directory requires confirmation
- cancelled confirmation performs no writes

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Add safe conflict handling for bean new
```

Review checklist:

- Preflight runs before writing files.
- `--force` scope is narrow and explicit.
- `.gitignore` is never overwritten.
- Error messages are actionable.
- There is no directory deletion or cleanup behavior.

---

## Phase 5 — Scaffold rendering and file creation

### Summary

Generate the actual project files and directories. Use stable templates and track what was created/replaced/skipped for the final summary.

### Implementation steps

1. Implement `templates.rs`.

Functions:

```rust
pub fn config_template(project_name: &str) -> String
pub fn page_template() -> &'static str
pub fn manifest_template() -> &'static str
pub fn gitignore_template() -> &'static str
pub fn gitignore_append_block() -> &'static str
```

2. Generate `#config.bst` with:

```bst
# project = "html"
# entry_root = "src"
# dev_folder = "dev"
# output_folder = "release"
# page_url_style = "trailing_slash"
# redirect_index_html = true
# name = "<project name>"
# version = "0.1.0"
# author = ""
# license = "MIT"
# html_lang = "en"
```

3. Generate `src/#page.bst` exactly as agreed.

4. Generate manifests:

```text
# beanstalk-manifest v2
# builder: html
# managed_extensions: .html,.js,.wasm
```

5. Generate directories:

```text
src/
lib/
dev/
release/
```

6. Implement `.gitignore` flow:

- If no `.gitignore`, prompt default yes and create it if accepted.
- If `.gitignore` exists, prompt default yes and append missing `/dev` block if accepted.
- If `.gitignore` already contains `/dev`, skip with clear report entry.
- Do not overwrite `.gitignore`.

7. File write order:

   1. create missing directories
   2. create/replace `#config.bst`
   3. create/replace `src/#page.bst`
   4. create/replace `dev/.beanstalk_manifest`
   5. create/replace `release/.beanstalk_manifest`
   6. create/append `.gitignore`

8. On write failure, return a precise error:

```text
Project creation failed while writing src/#page.bst: <io error>.
Some scaffold directories may already have been created. No existing files were overwritten unless --force was confirmed.
```

9. Add tests:

- creates full default scaffold in empty temp dir
- generated config exactly matches expected content
- generated page exactly matches expected content
- manifests are generated under `dev/` and `release/`
- `lib/` is created and empty
- `.gitignore` is created by default
- existing `.gitignore` gets append block when confirmed
- existing `.gitignore` is unchanged when declined
- existing `.gitignore` with `/dev` is not duplicated
- project name is escaped in config

10. Add an integration-ish unit test that runs the scaffold function, then invokes the existing project build/check path if practical. If this becomes awkward due to prompt abstraction or binary command boundaries, keep it as a unit-level generated-file assertion and rely on final `just validate`.

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Generate complete default HTML project scaffold
```

Review checklist:

- Templates are centralized, not scattered through write logic.
- Generated files have trailing newlines.
- No stale old `#project_name` key.
- No `full_path.join("../..")`.
- Existing user files are not overwritten unless force confirmed.
- `.gitignore` append is idempotent.

---

## Phase 6 — User-facing CLI output and error polishing

### Summary

Make the command explain exactly what happened. This is important because `new` is often a user’s first interaction with `bean`.

### Implementation steps

1. Add summary rendering in `mod.rs` or a small `summary.rs` if the output gets long.

2. Print:

```text
Created Beanstalk HTML project:
  Project path: /path/to/site
  Project name: site
```

3. Print sections only when non-empty:

```text
Created:
Updated:
Replaced:
Skipped:
```

4. Always print next steps:

```text
Next:
  cd /path/to/site
  bean check .
  bean dev .
```

5. If project path is the current directory, either omit `cd` or show:

```text
Next:
  bean check .
  bean dev .
```

6. Ensure cancellation paths are calm and explicit:

```text
Cancelled project creation.
```

7. Ensure conflict messages are specific.

8. Add tests for report construction if summary formatting is extracted into a pure function.

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Polish bean new output and cancellation messages
```

Review checklist:

- Output is explicit but not noisy.
- No vague raw IO errors reach the user without context.
- Summary accurately reflects created/replaced/skipped/updated paths.
- Cancellations are not treated as errors.

---

## Phase 7 — Documentation updates

### Summary

Update user-facing docs to match the new scaffold behavior.

### Files to update

```text
docs/src/docs/getting-started/#page.bst
docs/src/docs/project-structure/#page.bst
docs/roadmap/roadmap.md
docs/roadmap/plans/bean-new-command-hardening-plan.md
```

### `docs/roadmap/plans/bean-new-command-hardening-plan.md`

Create this plan file.

It should contain:

- current bug summary
- agreed command behavior
- scaffold tree
- generated files
- implementation phases
- deferred follow-ups
- validation command

This markdown file can be the same content as this implementation plan, adjusted if implementation details change during work.

### `docs/roadmap/roadmap.md`

Add a link under `# Plans / Notes / TODOS` while implementation is pending:

```md
- `bean new` command hardening: `docs/roadmap/plans/bean-new-command-hardening-plan.md`
```

Add deferred follow-ups:

```md
- `bean new` follow-ups: non-interactive `--yes`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.
```

After implementation lands, keep only the follow-ups if the hardening plan is complete.

### `docs/src/docs/getting-started/#page.bst`

Update:

1. Current commands table:
   - `bean new html [path] [--force]`
   - Mention interactive confirmations.
   - Mention generated starter page, `lib/`, manifests, and optional `.gitignore`.

2. “Create a new HTML project” section:
   - Remove instructions saying to manually add `src/#page.bst`.
   - Show examples:

```bash
bean new html
bean new html my-site
bean new html ~/projects/my-site
```

3. Document prompts:
   - current directory confirmation
   - existing directory choice
   - missing directories confirmation
   - project name prompt
   - `.gitignore` prompt

4. Document `--force`:
   - only replaces scaffold-owned files
   - still asks for confirmation
   - does not overwrite `.gitignore`

5. Update next steps:

```bash
cd my-site
bean check .
bean dev .
```

6. Mention generated project structure.

### `docs/src/docs/project-structure/#page.bst`

Update the project tree in the scaffold/example section to include:

```text
project-root/
├── #config.bst
├── src/
│   └── #page.bst
├── lib/
├── dev/
│   └── .beanstalk_manifest
├── release/
│   └── .beanstalk_manifest
└── .gitignore
```

Clarify:

- `lib/` is created empty by default.
- `dev/` and `release/` are output folders.
- `.beanstalk_manifest` is build-system metadata for safe stale-output cleanup.
- `src/#page.bst` is the default HTML page entry.

### Audit / style guide review / validation commit

Commit title suggestion:

```text
Document hardened bean new scaffold behavior
```

Review checklist:

- Docs match actual CLI prompts and generated files.
- No obsolete “add src/#page.bst manually” instructions remain.
- `codesnippet` is used for small inline code snippets in Beanstalk docs pages.
- Square brackets inside `codesnippet` are handled carefully because `codesnippet` does not escape them automatically.
- Roadmap includes deferred `--yes` and scaffold preset follow-ups.

---

## Phase 8 — Final audit and validation

### Summary

Run one final whole-repo validation pass and review for style-guide compliance.

### Final validation command

Use only:

```bash
just validate
```

No separate validation commands are needed unless `just validate` fails and a narrower command is useful for debugging.

### Final audit checklist

#### Behavior

- `bean new html` scaffolds into current directory only after confirmation.
- `bean new html path` treats `path` as the target path.
- Existing directories trigger explicit choices.
- Missing directories require confirmation.
- Project name prompt defaults to final directory basename.
- `.gitignore` default is yes.
- `--force` replaces only scaffold-owned files after a double-check warning.
- No unrelated user files are overwritten.
- No automatic `bean check` is run.

#### Generated files

- `#config.bst` uses `# project = "html"`.
- `#config.bst` uses one name key: `# name = "..."`
- `src/#page.bst` matches the agreed starter content exactly.
- `lib/` exists and is empty.
- `dev/.beanstalk_manifest` exists.
- `release/.beanstalk_manifest` exists.
- `.gitignore` is created/appended safely.

#### Code quality

- No `check_if_valid_path` in scaffold creation.
- No `../..` path creation.
- No `unwrap()`/`panic!` from user input.
- Module responsibilities are separated.
- Prompt flow is testable.
- Errors are precise and actionable.
- `mod.rs` acts as the structural map.

#### Docs

- Getting Started page matches implementation.
- Project Structure page matches scaffold.
- Roadmap links the plan and records deferred follow-ups.
- No stale docs contradict the new behavior.

### Final commit

Commit title suggestion:

```text
Harden bean new HTML project scaffolding
```

---

## Deferred follow-ups

Record these in `docs/roadmap/roadmap.md`:

```md
- `bean new` follow-ups: non-interactive `--yes`, template selection, project type aliases, richer scaffold presets, and optional package/dev tooling setup.
```

Potential future flags:

```bash
bean new html site --yes
bean new html site --template blog
bean new html site --no-gitignore
bean new html site --name "My Site"
```

Do not implement these in this pass.
