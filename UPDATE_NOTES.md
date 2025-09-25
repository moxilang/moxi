# Notes for Updating Moxi Language Guide

As we keep developing Moxi, the language guide (`MOXI_LANGUAGE.md`) should stay in sync with the codebase.  
Whenever features or syntax change, update the guide accordingly.

---

## Relevant Files in the Project (tied to language)
These files are the backbone of Moxi’s language layer:

```
# WHITE.md ready 👁️‍🗨️
src/parser.rs
src/types.rs
src/colors.rs
src/bevy_viewer.rs
src/viewer.rs
src/export.rs
src/main.rs
src/moxi/lexer.rs
src/moxi/parser.rs
src/moxi/runtime.rs
src/moxi/commands.rs
```
---

## Workflow for Updates
1. When new syntax/features are added in `src/moxi/`, reflect them in `MOXI_LANGUAGE.md`.
2. When stdlib grows (`translate`, `grid`, etc.), update the list in `MOXI_LANGUAGE.md`.
3. Keep examples in the guide runnable under current interpreter.


