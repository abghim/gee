# Gee -- Good Enough Emacs

Plans (Important!)
> [!NOTE]
> The most actively developed feature is syntax highlighting, under the feature/hl branch. The master branch receives less frequent updates.

I plan to implement the following before launching Gee 1.0
- Undo & file backup (robust, unlimited undo tree planned for the distant future)
- Search & replace

As for syntax highlighting, I have decided to develop it in a separate branch, feature/hl. **I have decided that the gee editor itself will remain a ~500KB binary with no syntax highlighting. As soon as 1.0-level development finishes, I will fork gee to a different editor and merge syntax highlighting.**


Emacs that's good enough. Sane defaults, including:
- tabs are displayed as four spaces. All tabs are converted to four spaces on read & save.
- always preserves indent
- nice C-like grammar editing with curly bracket completion & indent
- C-z for auto-save and exit (no suspend)
- no useless mode line
- ~500KB build size, unlike 7M emacs

Emacs features `gee` supports:
- navigation bindings: C-npbf, C-e, C-a
- save bindings: C-x C-s, C-z (this does NOT suspend, saves and exits)
- quit: C-x C-c
 => more to be added soon.

 ## Use of Generative A.I.
 ChatGPT 5.1 was used for minor debugging purposes to a limited extent throughout src/main.rs.
 OpenAI Codex was used to write multi-line-insert and delete logic.
 OpenAI Codex was used to Write Ctrl-d forward deletion logic.
