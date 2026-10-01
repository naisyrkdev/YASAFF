# Making an application for YASAFF

This guide explains how to build an app that YASAFF can install, list in its catalog, launch and hold inside its own window. It is based on the two existing apps, [calculator](calculator/) and [WitchyFiPlaya](WitchyFiPlaya/), and on the host code in [YASAFF/src/appstore.rs](YASAFF/src/appstore.rs) and [YASAFF/src/app/store_screen.rs](YASAFF/src/app/store_screen.rs).

For a ready-made starting point, copy [YasaffBlankApp](YasaffBlankApp/). It is an empty window held inside YASAFF, and its only control is Quit.

---

## 1. What a YASAFF app is

A YASAFF app is **any native executable**, shipped in a **`.zip` package** with a **`yasaffConfig.json`** manifest at its root. YASAFF doesn't load your code as a plugin. It starts your program as a child process and talks to it through:

- **environment variables**, set once at launch,
- **stdin**, where YASAFF sends you `show …` lines,
- **stdout**, where you send YASAFF the `yasaff:home` line.

That means any language works. Both existing apps use the same stack, though, and the blank app follows it:

| Crate    | Version | Used for |
|----------|---------|----------|
| `eframe` / `egui` | 0.36.2 | Window and immediate-mode drawing |
| `gilrs`  | 0.11.2  | Game controller input |
| `chrono` | 0.4     | Top-bar clock (optional) |

Every app has the same file layout:

```
my-app/
├── Cargo.toml
├── package.sh          # builds the release binary and writes dist/<id>-<version>.zip
├── README.md           # controls table, packaging notes
├── assets/
│   ├── icon.png        # catalog cover
│   └── background.png  # catalog background
└── src/
    ├── main.rs         # the window, drawing, handling commands
    ├── host.rs         # talks to YASAFF (copy it unchanged)
    └── pad.rs          # turns controller and keyboard input into Commands
```

---

## 2. The package (`.zip`)

The `package.sh` in each app produces this:

```
calculator-0.1.0.zip
├── yasaffConfig.json
├── icon.png
├── background.png
└── bin/
    ├── linux/calculator
    └── windows/calculator.exe     # only if WINDOWS_EXE was set
```

How YASAFF unpacks it (`unpack` and `install_package_in` in `appstore.rs`):

- `yasaffConfig.json` can sit at the zip root, **or** inside a single top-level folder (e.g. `calculator/yasaffConfig.json`). YASAFF strips that folder.
- `__MACOSX/` and `.DS_Store` are ignored. Paths that would escape the package are refused.
- The package is unpacked to a staging folder, validated, then swapped into `<YASAFF data>/apps/<id>/`, where `<YASAFF data>` is the active profile's folder. On Linux that's `~/.local/share/yasaff/profiles/<profile>/apps/<id>/`.
- Installing the same `id` again **replaces** the app (that's how updates work). You can't install over an app while it is running.
- On Unix, the `run.<platform>.command` file and every file listed in `install.executables` get `chmod +x`.
- Uninstalling deletes `apps/<id>/` but **keeps** `appdata/<id>/` (see §5).

---

## 3. The manifest: `yasaffConfig.json`

```json
{
  "yasaff": "app",
  "formatVersion": 1,
  "id": "calculator",
  "name": "Calculator",
  "version": "0.1.0",
  "kind": "app",
  "author": "Krystian Grzelak",
  "summary": "A simple calculator you use with a gamepad",
  "description": "Longer text.\n\nMention the controls here.",
  "icon": "icon.png",
  "background": "background.png",
  "run": {
    "linux":   { "command": "bin/linux/calculator" },
    "windows": { "command": "bin/windows/calculator.exe" }
  },
  "install": {
    "executables": ["bin/linux/calculator"]
  },
  "git": {
    "repository": "https://github.com/you/calculator",
    "branch": "main",
    "commit": "4ed5bf18…",
    "tag": null,
    "dirty": false
  }
}
```

| Field | Required | Rules / meaning |
|-------|----------|-----------------|
| `yasaff` | yes | Must be exactly `"app"`. |
| `formatVersion` | yes | Must be `1`. Any other value: "not supported; update YASAFF". |
| `id` | yes | 2–63 characters: `a-z`, `0-9` and `-`, not starting with `-`. It names the install folder and the data folder, and it is how YASAFF recognises an update. **Never change it after release.** Use the same string as the Cargo package name so the binary is `target/release/<id>`. |
| `name` | yes | Display name in the catalog. |
| `version` | yes | Any string. A different version from the store counts as an update. |
| `kind` | yes | `"app"` or `"game"`. `"game"` puts the entry with games in the library. |
| `author`, `summary`, `description` | no | Shown in the catalog. `description` can contain `\n\n` paragraphs. |
| `icon` | no | Path inside the package. Becomes the library **cover**, unless the user already set one. |
| `background` | no | Path inside the package. Becomes the library **background**. 1920×1080 works well. |
| `artwork` | no | More library images, each a path inside the package: `cover` (portrait cover for the **Game covers** tiles and the front of the 3D boxes), `squareCover` (cover for the square tiles: **Square cards**, 3D objects, discs and CRT screens), `booklet` (portrait front of a 3D case), `spine`, `top`, `bottom`, `back`, `side` (right side of the 3D box), and `logo` (used only by `builtin`). Like `icon`, each is set only where the user has none. With `cover` or `squareCover`, YASAFF keeps one cover per shape and shows the one that fits the tile style; a shape without its own uses `icon`. A cover the user picks replaces only the shape shown at the time. |
| `builtin` | no | `"mock-game"`: the package has no program. YASAFF opens a window with `artwork.logo` and a note that the game can be deleted. `run` isn't needed. The games in [initial_content/](../initial_content/) use it. |
| `run` | yes (unless `builtin`) | A map from platform (`linux`, `windows`, `macos`) to a run entry. The running platform **must** have an entry, or the package is refused ("can't run on linux"). |
| `run.<p>.command` | yes | Relative path to the executable. It **must exist** in the package. |
| `run.<p>.args` | no | List of extra command-line arguments. |
| `run.<p>.workingDir` | no | Relative working directory. Defaults to the install folder. |
| `install.executables` | no | Extra files that get `chmod +x` on Unix (helper binaries, scripts). |
| `git` | no | Informational, but `git.commit` also feeds update detection: when the versions are equal and the commits differ, YASAFF offers an update. |

All paths (`command`, `workingDir`, `icon`, `background`, `artwork.*`, `executables`) must be relative and stay inside the package. `..` and absolute paths are rejected.

---

## 4. Runtime contract: how YASAFF runs your app

### 4.1 Environment variables

| Variable | Value | What to do with it |
|----------|-------|--------------------|
| `YASAFF` | `1` | You're running inside YASAFF. Enable the stdin/stdout protocol. |
| `YASAFF_APP_ID` | your `id` | Optional. |
| `YASAFF_APP_VERSION` | your `version` | Optional. |
| `YASAFF_APP_DATA` | absolute path, already created | **Store all settings and saves here** (see §5). |
| `YASAFF_FULLSCREEN` | `1` | YASAFF is fullscreen. Open fullscreen, without decorations. |
| `YASAFF_WINDOW` | `"x y w h"` (floats, screen coordinates) | YASAFF is windowed. Open **without decorations, not resizable**, exactly over YASAFF's inner rect. That is what makes the app look "held inside" YASAFF. |

When neither placement variable is set (placement `free`), open a normal window. The same happens when the app is started outside YASAFF, so every app must also work **on its own**. The `--fullscreen` command-line flag forces fullscreen.

### 4.2 stdin: messages from YASAFF

YASAFF writes one line at a time:

```
show fullscreen
show window 100 80 1280 720
show free
```

YASAFF sends `show` when the user picks your (already running) app again in its library. Your app must then:

1. un-minimize,
2. switch fullscreen on or off to match,
3. for `window`, resize to `w h` and move to `x y`,
4. take focus.

For `free`, stay wherever you are.

### 4.3 stdout: messages to YASAFF

| Line | Effect |
|------|--------|
| `yasaff:home` | YASAFF un-minimizes and takes focus. Send it when the user minimizes your app (Guide button), right after you minimize yourself. |
| anything else | Passed through to YASAFF's own stdout, so plain `println!` logging is harmless. |

Flush stdout after `yasaff:home`, because stdout is a pipe and doesn't flush on newlines.

### 4.4 Lifecycle

- **Launch:** YASAFF shows its launching screen and starts you. Only one store app can run at a time.
- **Minimize / "back to YASAFF":** you minimize yourself and print `yasaff:home`. Your process keeps running (WitchyFiPlaya keeps playing music).
- **Resume:** YASAFF sends `show …` (see 4.2).
- **Quit:** just exit the process (close the window). YASAFF sees that the child exited, un-minimizes and focuses itself.
- **Forced close:** *Options → Close application* in YASAFF **kills** the process, so don't rely on shutdown code running. Save state as it changes.
- **Crash detection:** if you exit with a **non-zero status within 5 seconds** of launch, YASAFF shows "<name> closed right after starting (…)".

### 4.5 `host.rs`: the ready-made implementation

[calculator/src/host.rs](calculator/src/host.rs) implements all of §4.1 to §4.4 for eframe, and WitchyFiPlaya uses the exact same file. **Copy it unchanged** and wire it up like this:

```rust
fn main() -> eframe::Result {
    let host = Host::from_env();                       // reads YASAFF, YASAFF_WINDOW, ...
    let options = eframe::NativeOptions {
        viewport: host.viewport(                       // fullscreen / borderless at x,y,w,h / free
            egui::ViewportBuilder::default()
                .with_title("My App")
                .with_app_id("my-app")
                .with_inner_size([1280.0, 720.0])
                .with_min_inner_size([640.0, 360.0]),
        ),
        ..Default::default()
    };
    eframe::run_native("My App", options, Box::new(|cc| Ok(Box::new(App::new(cc, host)))))
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, mut host: Host) -> Self {
        host.listen(&cc.egui_ctx);                     // starts the stdin reader thread
        /* ... */
    }
}

impl eframe::App for App {
    // `logic` also runs while minimized, so `show` is handled even then.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.host.update(ctx);                         // applies `show`, keeps the window snapped to YASAFF
        if ctx.input(|i| i.viewport().minimized) == Some(true) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) { /* ... */ }
}

// Quit:     ctx.send_viewport_cmd(egui::ViewportCommand::Close)
// Minimize: self.host.minimize(ctx)   // minimizes and prints yasaff:home
```

What `Host::update` also does: in `window` placement it checks every 250 ms whether the window manager moved or resized the window, and **snaps it back** onto YASAFF's rect.

---

## 5. Saving data

Read and write settings in `YASAFF_APP_DATA`. If it isn't set, fall back to a normal per-user folder so the app also works on its own. From [WitchyFiPlaya/src/config.rs](WitchyFiPlaya/src/config.rs):

```rust
fn dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("YASAFF_APP_DATA") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "my-app")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}
```

YASAFF keeps this folder when the app is updated or uninstalled, and registers it as the entry's save folder.

---

## 6. UI and controller conventions

YASAFF is a couch launcher, so apps should be **fully usable with a game controller** and should look like they belong to YASAFF.

### 6.1 Button mapping (gilrs names)

| Button | gilrs | Convention |
|--------|-------|------------|
| A | `South` | Confirm / press highlighted item |
| **B** | `East` | **Quit / back** |
| X | `West` | Secondary action (delete, pause) |
| Y | `North` | Tertiary action (clear, shuffle) |
| LB / RB | `LeftTrigger` / `RightTrigger` | Previous / next, − / + |
| LT / RT | `LeftTrigger2` / `RightTrigger2` | Secondary pair (volume, ÷ ×) |
| View | `Select` | Toggle |
| Menu | `Start` | Main secondary action |
| **Guide** | `Mode` | **Minimize → back to YASAFF** |
| D-pad / left stick | `DPad*`, `Axis::LeftStick*` | Move highlight (threshold 0.5, repeat after 380 ms, then every 90 ms) |

Also give every action a **keyboard** key (Esc = quit, M = minimize, arrows, Enter/Space), and make on-screen buttons **clickable with the mouse**.

### 6.2 Things that are easy to get wrong

- **Ignore controller input while unfocused.** YASAFF and your app read the same controller. If you don't check `ctx.input(|i| i.focused)`, a B press in YASAFF quits your hidden app. Still drain `gilrs.next_event()` every frame, but drop the events.
- **Keep repainting.** Gamepad events don't wake egui up. Call `request_repaint_after` every frame: about 16 ms while something animates or a button is held, 50 ms when focused, 250 ms when unfocused.
- **There's no title bar inside YASAFF.** Draw your own minimize button (top right, ▁) and quit hint. When `host.inside_yasaff` is true, label them "Back to YASAFF".
- **Scale everything** from a 1080p design: `let s = (screen.height() / 1080.0).max(0.4);`, then multiply every size and offset by `s`.

### 6.3 Look

The existing apps share one style:

- dark visuals (`egui::Visuals::dark()`),
- a purple vertical gradient (`#2a164c` → `#080512`),
- the title at top left and a clock at top right, both at y = 55·s,
- a **hint bar** at bottom right (`y = bottom − 45·s`) with coloured pills: A green `#5cb83c`, B red `#d93a3a`, X blue `#2f7df6`, Y yellow `#e8b924`, other buttons gray.

---

## 7. Step by step: a new app from the blank template

1. **Copy** `YasaffBlankApp/` to `my-app/`. Delete `target/`, `dist/` and `Cargo.lock` from the copy.
2. **Rename** the id `blank-app` → `my-app` in:
   - `Cargo.toml`: `name = "my-app"`,
   - `package.sh`: `id="my-app"`, and update `name`, `summary`, `description`, `author` in the JSON block,
   - `src/main.rs`: `.with_title(…)`, `.with_app_id("my-app")`, the `run_native` name.
3. **Replace** `assets/icon.png` (square, e.g. 512×512) and `assets/background.png` (1920×1080).
4. **Add commands** to `pad.rs` (`Command` enum, `button_command`, `keyboard`). Handle them in `App::handle`. Map Guide (`Button::Mode`) and `M` to `self.host.minimize(ctx)`, and remove the `#[allow(dead_code)]` from `host.rs`.
5. **Test on its own:** `cargo run`.
6. **Test YASAFF placement without YASAFF:**
   ```bash
   YASAFF=1 YASAFF_WINDOW="100 100 1280 720" cargo run
   # then type on stdin:   show window 200 150 1280 720   (Enter)
   ```
7. **Package:** `./package.sh` writes `dist/my-app-<version>.zip`.
   Add a Windows build with `WINDOWS_EXE=path/to/my-app.exe ./package.sh`.
8. **Install:** in YASAFF use **Add → Import application from file…** and pick the zip. The file window opens in `apps_installation_packs` in the profile's data folder (YASAFF creates it), a handy place to keep app packages. The app appears in the library with your icon and background.
9. **Update:** bump `version` in `Cargo.toml`, re-run `package.sh`, import again. The same `id` replaces the old install and keeps its data.
10. **Publish (optional):** upload the zip to a YASAFF Discover Server ([YasaffStuffServer](YasaffStuffServer/), *My apps*). YASAFF lists it in Discover for the platforms in `run`, and checks the download against its sha256.

---

## 8. Checklist before release

- [ ] `id` is valid (lowercase, digits, `-`) and equals the Cargo package name
- [ ] `run.linux.command` (and `run.windows.command` if shipped) exists in the zip
- [ ] The app opens borderless over YASAFF with `YASAFF_WINDOW` set, and fullscreen with `YASAFF_FULLSCREEN`
- [ ] `show …` on stdin brings it back from minimized
- [ ] Guide / M minimizes **and** prints `yasaff:home` (flushed)
- [ ] B / Esc quits (exits the process)
- [ ] Controller input is ignored while unfocused
- [ ] Settings live in `YASAFF_APP_DATA` and are saved as they change (the app can be killed)
- [ ] Startup errors don't exit non-zero within 5 s (or, if they do, that's intended)
- [ ] It works on its own too (no `YASAFF` variable)
- [ ] README has a controller + keyboard controls table

---

## 9. Addons: bar widgets

An **addon** extends YASAFF itself instead of running as an app. The first kind is the **bar widget**: a small background program whose output YASAFF draws in its top or bottom bar. The example is [addons/DeviceInfoDisplayer](addons/DeviceInfoDisplayer/), and it is a good one to copy.

### 9.1 Package

The same `.zip` + `yasaffConfig.json` as an app, with three differences:

```json
{
  "yasaff": "addon",
  "kind": "bar-widget",
  "widget": { "minHeight": 110 },
  ...
}
```

| Field | Meaning |
|-------|---------|
| `yasaff` | `"addon"`. Older YASAFF versions refuse the package instead of installing it as an app. |
| `kind` | `"bar-widget"` (the only addon kind so far). |
| `widget.minHeight` | Optional. The bar height (at 1080p, 40–300) the widget needs. When the user's bar is lower, YASAFF raises it, and Settings shows *"Height · <addon> needs at least N px"*. |
| `widget.images` | Optional. Images the user can replace: `[{"key": "face", "label": "Clock face"}, …]`. The addon's page in the Addons Manager shows a *Choose…* row for each and a *Use built-in images* row. The picked file is copied to `$YASAFF_APP_DATA/images/<key>.<png\|jpg\|jpeg\|webp>` and the widget is restarted, so read it at startup and fall back to your own image when it's missing. `key` uses the same characters as `id`. |

`run`, `install`, `icon` and `git` work as for apps. `background` is not used. YASAFF installs addons into `<YASAFF data>/addons/<id>/`; they don't appear in the library.

### 9.2 Placing it

Installing (Store → **Addons** tab, or **Settings → Addons → Addons Manager…** → *Install addon…*) doesn't show the widget yet. *Install addon…* lists the packs (`.zip` files) in `addons_installation_packs` in the profile's data folder, next to `addons/`: open one to *Install* it or *Delete pack* (only the `.zip` is deleted; the installed addon stays). *Import new pack…* at the bottom opens the file window in that folder, copies the chosen `.zip` there (replacing a pack with the same file name) and installs it. In the Addons Manager, *Installed addons* lists every addon; open one and use Left/Right on *Shown in* to pick **Top bar**, **Bottom bar**, **Anywhere on screen** or **Off**. *Size* (50–300 %) scales the whole widget; in a bar, `minHeight` grows with it. Anywhere on screen adds *Position on screen → Move…*: the arrows or a mouse drag move the widget, the mouse wheel resizes it, Confirm keeps the change and Back undoes it. That page also has the addon's replaceable images and *Uninstall*. After an import, YASAFF opens the new addon's page. Widgets are centered in their bar. The placement is saved in `userSettings.json` (`addons.<id>`: `bar`, `on_screen`, `x` and `y` as fractions of the screen for the widget's center, and `size` in percent). On the screen, a widget is as tall as its `minHeight` (110 px if it has none) times its size.

### 9.3 Runtime contract

While the widget is placed, YASAFF keeps its `run` command running in the background (no console window on Windows):

| Variable | Value |
|----------|-------|
| `YASAFF` | `1` |
| `YASAFF_ADDON_ID` | the addon's `id` |
| `YASAFF_APP_DATA` | data folder, as for apps |
| `YASAFF_WIDGET_BAR` | `top`, `bottom` or `screen` (anywhere on screen) |

- **stdout:** print `yasaff:widget {"items":[…]}` lines and flush. YASAFF shows the newest line. Each item is `{"label": "CPU", "value": "12 %", "level": 0.12, "color": "#5cb83c"}`, where `level` (0–1) and `color` are optional. With `level`, YASAFF draws a meter that is green, yellow or red unless `color` is set. An item with `"layers": [{"image": "assets/face.png", "angle": 0}, …]` is drawn as a square **dial** instead: the images stacked in order, each fitted into the square and turned clockwise by `angle` degrees about its center. Relative image paths are relative to the addon's folder. [addons/ClockFace](addons/ClockFace/) uses it. Other lines are only logged.
- **stdin:** YASAFF writes nothing. It **closes** stdin when the widget is turned off or YASAFF quits, so exit on EOF. YASAFF also kills the process.
- **Crash:** if the program exits, the bar shows the error and YASAFF starts it again after 10 seconds.
- Before an update is installed from the store, YASAFF stops the widget and starts the new version afterwards.

### 9.4 Publishing

Upload the zip on the YASAFF Discover Server like an app (**Upload → An addon**). It's listed under **Addons** on the website and in YASAFF Discover, at `GET /api/v1/addons?platform=…`. That endpoint has the same shape as `/apps`, plus `minHeight`. `/apps` doesn't list addons.
