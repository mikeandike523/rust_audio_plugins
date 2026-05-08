# CLAUDE.md

Development tips and instructions

## Dev environment tips

- Use the repos_to_explore folder if you have trouble finding READMEs or examples for dependencies
- Use the "scratchpad" folder to play around with different concepts or test if code works
- These folders prevent polluting the main folder with artifacts or accidentally embedding inner repos
- The main tooling in this repo is pnpm (node) and cargo (rust), nothing else
- You (Claude Code) are running in a fully equipped native Windows host environment. So:
    1. Node and cargo builds should work normally
    2. Avoid installing new packages unless necessary
- Do NOT do any computer-wide manipulations such as npm install -g

## Branch-based project focus

Run `git branch --show-current` at the start of a session to determine the active branch.

If the branch name matches the pattern `development.plugins.<plugin_name>`, then:
- The primary working directory for that session is `<repo_root>/custom_plugins/<plugin_name>`
- Avoid touching monorepo-wide files unless a change is genuinely required across the whole repo
- Prefer scoping reads, edits, and searches to that plugin's folder first

## Plugin project structure

Most plugins have a web-based GUI. A plugin project is a valid Rust project, but may contain an additional subfolder:

```
custom_plugins/<plugin_name>/
  src/                  # Rust source
  Cargo.toml
  web-gui/              # Web-based GUI (if present) — developed separately from the Rust code
    ...
```

When working on GUI-related tasks, look inside `web-gui/`. When working on DSP/audio logic, look in `src/`.

## Reading context before large changes

Before starting any large change or new feature request, read recent commits for context:

```
git log --oneline -20
```

This helps avoid duplicating work, contradicting recent decisions, or missing relevant in-progress state. For especially significant changes, also skim the diffs of the most recent commits.

### Discovering recent merges and understanding branch history

Plugin branches often have unrelated changes merged into them (e.g. `Merge branch 'main' into development.plugins.foo`). To see the full picture including merge topology:

```
git log --oneline --graph -20
```

To focus only on commits authored directly on the current branch (ignoring merged-in side history):

```
git log --oneline --first-parent -20
```

Use `--graph` to spot merge commits and identify where unrelated work was folded in, then use `--first-parent` to filter down to what was actually developed on this branch. This helps distinguish the plugin's own progress from incidental merges.

## Validating plugin changes

After making changes to a plugin, run both of these checks before considering the work done:

```bash
# 1. Build the web GUI (if the plugin has one)
cd custom_plugins/<plugin_name>/web-gui && pnpm run build

# 2. Compile and bundle the Rust release build (from repo root)
pnpm run <plugin_name>:rust:bundle-release
```

Both must succeed with no errors (pre-existing warnings in nih_plug / nih_plug_webview are fine to ignore).

If the bundle step fails with "cannot overwrite executable", the plugin is loaded in the DAW. The user will close the DAW and re-run the bundle manually — no action needed from Claude.
