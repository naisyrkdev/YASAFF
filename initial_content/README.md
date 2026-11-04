# Initial content

What a new YASAFF profile is offered on its first start. Nothing is bundled: the packages come from GitHub.

## initial_install_repos.txt

[initial_install_repos.txt](initial_install_repos.txt) lists GitHub repositories, separated by `;`:

```text
naisyrkdev/YasaffDoubleAxisTheme;
someone/SomeYasaffAddon;
someone/SomeYasaffGame
```

- Write each repository as `owner/name`, or paste its `https://github.com/owner/name` address.
- Spaces and line breaks around entries don't matter, so one per line is fine. The `;` after the last one is optional.
- An entry that isn't a repository is skipped with a warning in the log, and a repeated one is offered once.
- The list is embedded at compile time, so run `cargo build` after changing it. `cargo test` checks that every entry is a repository.

## How it works

1. When a profile starts with an empty library and hasn't been set up yet, YASAFF shows the welcome screen and looks up each repository on GitHub, without downloading it: its description, and its type from the files at the root of its default branch (`themeSettings.json` makes it a **theme**, `yasaffConfig.json` an **addon**, **game** or **app**; a repository with neither shows **app**).
2. Back on the home screen, it asks about them one at a time, in the order of the list: **Do you want to download this repo?** `(theme) naisyrkdev/YasaffDoubleAxisTheme - Double axis theme for YASAFF Project`.
3. **Download** installs it as **Discover → Installed → Install from GitHub…** does: the `.zip` of the latest release, or the default branch's source when there is no release. A theme is added as a new theme and selected. **Skip** moves on to the next one.
4. The profile then gets an `initialContent.json` marker, so the questions aren't asked again. A new or reset profile gets them again. A repository that can't be looked up (no connection, or GitHub's limit of 60 requests an hour) is skipped with a message.

To be offered, a repository must be public and hold a YASAFF package: a release `.zip`, or, without a release, a default branch that needs no build. See [docs/YASAFF_application_guide.md](../docs/YASAFF_application_guide.md) and [docs/THEMES.md](../docs/THEMES.md).
