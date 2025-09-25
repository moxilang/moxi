# Notes for Updating Mochi Language Guide

As we keep developing Mochi, the language guide (`MOCHI_LANGUAGE.md`) should stay in sync with the codebase.  
Whenever features or syntax change, update the guide accordingly.

---

## Relevant Files in the Project (tied to language)
These files are the backbone of Mochi’s language layer:

```
# WHITE.md ready 👁️‍🗨️
src/parser.rs
src/types.rs
src/colors.rs
src/bevy_viewer.rs
src/viewer.rs
src/export.rs
src/main.rs
src/mochi/lexer.rs
src/mochi/parser.rs
src/mochi/runtime.rs
src/mochi/commands.rs
```
---

## Workflow for Updates
1. When new syntax/features are added in `src/mochi/`, reflect them in `MOCHI_LANGUAGE.md`.
2. When stdlib grows (`translate`, `grid`, etc.), update the list in `MOCHI_LANGUAGE.md`.
3. Keep examples in the guide runnable under current interpreter.


