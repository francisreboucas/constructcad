# Contributing to ConstructCraft

Thank you for your interest in ConstructCraft! Bug reports, feature ideas, translations, documentation and code are all welcome.

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to contribute

- **Report a bug** or **request a feature** using the [issue templates](https://github.com/francisreboucas/constructcraft/issues/new/choose).
- **Translate** the interface: see [Translations](#translations).
- **Improve documentation**: README, code comments, examples.
- **Write code**: issues labeled `good first issue` and `help wanted` are a good start. For larger changes, open an issue first so the design can be discussed before you invest time.

Questions and ideas that are not bugs belong in [Discussions](https://github.com/francisreboucas/constructcraft/discussions) when they are enabled, or in an issue otherwise.

## Clean-room policy

ConstructCraft is an independent implementation. To keep it that way, every contribution must follow these rules:

- **Do not** submit code, data, assets or text copied or derived from proprietary software, including decompiled or disassembled binaries, leaked source code or documents, and SDK samples whose license does not allow redistribution under the GPL.
- **Do not** submit content obtained in violation of an EULA, NDA or employment agreement.
- Behaviour may be designed from **public documentation**, **general CAD/BIM conventions**, **open standards** (such as IFC) and **user feedback**.
- Reference tests must use **analytic values** or results from **open-source tools**, never output captured from proprietary software.
- Do not use third-party trademarks in names, icons or the interface in a way that suggests affiliation.

If you are unsure whether something is allowed, ask in an issue before submitting it.

## Developer Certificate of Origin

All commits must be signed off under the [Developer Certificate of Origin 1.1](https://developercertificate.org/), which certifies that you wrote the contribution or otherwise have the right to submit it under the project's license. Add the sign-off with:

```sh
git commit -s
```

This appends a line like `Signed-off-by: Your Name <you@example.com>` to the commit message. It must match the commit author exactly, so check `git config user.name` and `git config user.email` first. Any name you are known by is fine, but not an anonymous one.

**Forgot to sign off?** The *DCO sign-off* check on your pull request lists the commits without it. Sign off the commits in your branch and update the pull request:

```sh
git fetch https://github.com/francisreboucas/constructcraft.git main
git rebase --signoff FETCH_HEAD   # adds the line to every commit since main
git push --force-with-lease
```

For only the last commit, `git commit --amend --no-edit -s` followed by `git push --force-with-lease` is enough. Commits made in the GitHub web editor are signed off automatically.

If you cannot rewrite your branch, say in the pull request that you agree to the DCO for its commits and a maintainer can sign them off on your behalf when merging.

## Development setup

1. Install a recent stable Rust toolchain with [rustup](https://rustup.rs/) (the workspace uses edition 2024).
2. Fork and clone the repository.
3. Build and run:

   ```sh
   cargo run -p cc_app
   ```

On Linux, eframe/wgpu need the usual windowing and graphics development packages (for example X11/Wayland and Vulkan or OpenGL drivers).

## Before opening a pull request

Run the same checks as CI. All of them must pass:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Project rules

- **One model, many views.** Every change to the model goes through `Document::edit` so it is recorded as a reversible `Transaction`. Never mutate the document outside a transaction.
- **Robustness.** Non-test code must not panic (no `unwrap`/`expect`/`panic!`; clippy denies them outside tests) and must not use `unsafe`. Treat data read from files as hostile: `Project::repair` cleans projects on load, indices from external data use `get()`, recursion over model references is bounded. Crash fixes come with a regression test that failed before the fix. See AGENTS.md.
- **Dialogs.** Build every dialog with `cc_app::dialog::Dialog`: title and close button in the header, buttons in the footer (main action first, Cancel last, auxiliary buttons on the left, destructive actions with `danger`). Dialogs that edit data offer OK / Cancel and wrap their edits in `dialog::session` so Cancel undoes them. Colors come from `theme::PALETTE`.
- **No hard-coded interface text.** User-facing strings go in `assets/i18n/*.ftl` and are accessed with `t!`. Add every new key to **all** languages; a test enforces matching keys and placeholders.
- **File format compatibility.** `.ccad` files are versioned. Changes to persisted structures must keep older files loadable or bump the format version with a migration.
- **Respect crate boundaries.** `cc_core` does not depend on geometry, views or UI; `cc_geom`, `cc_views`, `cc_render` and `cc_ifc` do not depend on `cc_app`.
- **Tests.** Bug fixes include a regression test where practical; geometry and schedule code is tested against analytic values.
- **Style.** Code is formatted with `rustfmt` (see `rustfmt.toml`). Code, comments, identifiers and commit messages are written in English.
- **Dependencies.** New dependencies must be GPL-3.0-compatible and justified in the pull request.
- **IFC.** IFC support is implemented in `cc_ifc` without third-party IFC libraries. Changes to the export must keep `scripts/validate_ifc.py` passing; IfcOpenShell is used only for that validation, never as a build or runtime dependency.

## Commits and pull requests

- Keep pull requests focused on one change; split unrelated work.
- Write commit messages in the imperative mood with a short summary line (≤ 72 characters), for example `Add hip roof generation`.
- Describe what changed and why, link the related issue (`Fixes #123`) and include screenshots for visual changes.
- Be responsive to review; maintainers may ask for changes before merging.

## Translations

1. Copy `assets/i18n/en.ftl` to `assets/i18n/<locale>.ftl` (for example `nl.ftl`).
2. Translate the values, keeping keys and `{ $placeholders }` unchanged.
3. Register the language in `Language` in `crates/cc_engine/src/i18n.rs` (`ALL`, `code`, `native_name`, `from_code`, `source`, and `decimal_comma` if it uses a decimal point).
4. Run `cargo test --workspace` to verify keys and placeholders.

## License

By contributing, you agree that your contributions are licensed under the [GNU General Public License v3.0 or later](LICENSE).
