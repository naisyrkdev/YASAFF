# YASAFF Launcher
Yasaff launcher is kind of "layer" that should allow your PC to behave more like console. Trying aim to seamless experience for users.

Trying to make it fully customizable ;)

![YASAFF demo](assets/for_docs/1.gif)

YASAFF is a console-style game launcher for PC, made to be used with a controller. It runs on Windows and Linux.

- [Features](#features)
- [Installing and building](#installing-and-building)
- [Adding games](#adding-games)
- [Controls](#controls)
- [Apps and games in the background](#apps-and-games-in-the-background)
- [Game options](#game-options)
- [Customization](#customization)
- [Discs, USB sticks and SD cards](#discs-usb-sticks-and-sd-cards)
- [Saves](#saves)
- [YASAFF Discover, apps and addons](#yasaff-discover-apps-and-addons)
- [Profiles](#profiles)
- [Settings overview](#settings-overview)
- [Data folders](#data-folders)
- [Logs](#logs)

## Features

- A fullscreen library you use with a controller. Keyboard and mouse work too.
- Finds installed games in **Steam**, **Heroic Games Launcher** and desktop shortcuts, and uses their artwork.
- Starts games directly or through their store. YASAFF follows the game process, so it knows when the game is running and comes back when the game closes.
- Two layouts (**Row view** and **Grid view**), a **Bubbles** theme and eight tile styles, from flat cards to 3D boxes, opening cases, spinning discs and CRT screens.
- Themes with their own backgrounds, bars, button looks, icons, fonts and sounds. You can export and import them.
- **Physical games:** insert a disc, USB stick or SD card that holds a `.yasaff` file and its game starts, like on a console. YASAFF can also burn that file to a CD or DVD.
- Save backups, and save syncing to the USB stick or SD card a game was started from.
- A virtual mouse on the right analog stick and button remapping.
- **YASAFF Discover** for apps, games, themes and addons, and bar widgets you can place on the screen.
- Several profiles, each with its own library and settings.
- Available in English, French, Spanish and Polish.

## Installing and building

**Windows:** run the `YASAFF-Setup-<version>.exe` installer. It installs for the current user and doesn't need admin rights.

**From source** (Rust, edition 2024):

```sh
cargo build --release
./target/release/yasaff               # add --fullscreen to start fullscreen
```

To build the Windows installer, run `installer/build-installer.ps1`. It needs Inno Setup 6.3 or newer and writes `target/installer/YASAFF-Setup-<version>.exe`.

Optional programs:

| Program | Needed for |
|---------|------------|
| `ffmpeg` | Preview and idle videos (Bubbles theme and CRT screens) |
| `xorriso`, or `wodim`/`cdrecord` + `genisoimage` for CDs, or `growisofs` for DVDs (Linux) | **Burn to CD/DVD…** |

Command-line flags:

| Flag | Effect |
|------|--------|
| `--fullscreen` | Start fullscreen, whatever the setting says |
| `--disc <folder>` | Treat `<folder>` as an inserted disc and read its `.yasaff` file |

## Adding games

A new profile starts with a few placeholder games, so the screen isn't empty. You can delete them.

Open the **Add game** tile, or press **Y** / **Insert**. From there you can:

- **Scan for games:** looks in Steam, Heroic and game shortcuts. Tick the games you want and import them. YASAFF also looks for images of the ticked games on this computer.
- **Add a file:** a game executable, a script, or a `.desktop` / shortcut file.
- **Add a command or URL:** for example `steam://rungameid/...` or any command line.
- **Import an application from file:** a YASAFF app or game package (`.zip`).

Games installed from YASAFF Discover are added to the library automatically.

## Controls

These are the default bindings. You can remap every controller button in **Settings → Controller**.

| Action | Controller | Keyboard |
|--------|------------|----------|
| Move | D-pad / left stick | Arrow keys |
| Select / confirm | A | Enter, Space |
| Back / cancel | B | Esc, Backspace |
| Delete | X | |
| Add / extra (Add game on the home screen) | Y | Insert |
| Options (game menu) | Menu | Tab |
| Toggle fullscreen | View | F11 |
| Bring YASAFF to front | Guide | |
| Mouse click / right-click (virtual mouse) | RT or R3 / LT | |

While a game or app is running, press **Guide** to come back to YASAFF. See [Apps and games in the background](#apps-and-games-in-the-background).

## Apps and games in the background

![Re-entering an app running in the background](assets/for_docs/bg_apps_1.gif)

You don't have to close a game or app to get back to YASAFF. Press **Guide** and it stays open in the background while YASAFF comes to the front. A music player, for example, keeps playing.

- The running entry shows **Running** under its name in the library.
- Select it and press **A**. Its options show **Switch to game** / **Switch to app** instead of **Start**. It opens again where you left it, fullscreen or over YASAFF's window, just as it was.
- To end it from YASAFF, use **Close game** / **Close application** in its options.
- When the game or app quits on its own, YASAFF comes back to the front by itself.
- Only one app from YASAFF Discover can run at a time.

## Game options

Press **Menu** (or **Tab**) on a game to open its options:

- **Start**, **Close game** / **Close application**, and **Update** for store apps.
- **Tile and box appearance…**: cover, spine, top, back, side and bottom images, the instruction booklet, a 3D model, preview and idle videos, colors and the title font, all for this game only.
- **Set cover image**, **Reposition cover image…**, **Set background image**, **Fetch artwork**.
- **Rename**, **Edit launch command**.
- **Swap position with another game…** (or **Swap with other bubble**), and **Mark this position as static for this game**.
- **Set save folder…**, **Open save folder**, **Back up saves…**.
- **SD and USB Yasaff settings…**: see [Discs, USB sticks and SD cards](#discs-usb-sticks-and-sd-cards).
- **Create disc file…**, **Burn to CD/DVD…**.
- **Remove from library**: removes only the library entry and its images. The game files stay.

## Customization

![YASAFF customization](assets/for_docs/customization_1.gif)

Everything about the look is part of a **theme**. Open **Settings → Theme** to switch themes, create one (it starts as a copy of the current one), rename, export, import or delete it. A theme is a folder with `themeSettings.json` and its images, so you can share it. The built-in themes can't be deleted, and the pre-defined **Bubbles** theme offers fewer options.

**Home screen**

- **Layout:** *Row view* or *Grid view*. Each layout keeps its own setup.
- **Tile style:** *Square cards*, *Game covers*, *Flat 3D boxes*, *3D model boxes*, *3D objects*, *3D discs*, *3D opening cases* or *CRT screens*.
- **Row view:** up to 4 rows, tile size, space between tiles, a bigger selected tile, and the row position (top, middle or bottom).
- **Grid view:** columns, rows per page, spacing, a bigger selected tile and the grid position.

**Tile styles**

- **Boxes and covers:** box shape (three preset sizes or a custom one from 50 to 400 mm), depth, spine label, spine, spine text and top colors. Each game can change them in its options.
- **Discs:** CD, DVD or Blu-ray look, angle, spinning, spin direction (around its center, flipping over, or flat like in a drive) and speed.
- **3D objects:** a game shows its own 3D model (STL, OBJ, PLY, glTF/GLB or 3MF), or its name in 3D letters.
- **CRT screens:** screen size, a CRT filter (scanlines, flicker, darker edges), and preview and idle videos.
- **Bubbles:** bubble size, random size difference, empty bubbles that fill the screen, shuffling, preview video, and a swap sound.

**Background:** each game's own background art, or a solid color, gradient or image (fill, fit, stretch, repeat, repeat mirrored or center).

**Top and bottom bars:** turn them on or off and set the height and fill: a color (`#rrggbbaa` makes it see-through), a gradient, or an image you can scale, move and rotate.

**Buttons:** use controller images or drawn buttons, choose the shape and colors per button, or use your own image. Button hints follow the controller you use (Xbox or PlayStation), or you can choose one in **Settings → Controller**.

**Tile icons:** use your own SVG icons for the Discover, Add game and Settings tiles.

**Sounds:** a tick when moving between games and a bloop between bubbles. You can use your own WAV, MP3, OGG or FLAC file.

**Fonts:** import TrueType or OpenType fonts (`.ttf`, `.otf`, `.ttc`) and use them for game titles, for all games or per game.

**Shortcuts:** give **Discover** and **Add game** a controller button, then show them as a library tile or in the top or bottom bar.

**Game order:** put the last played games first, or turn on a static order so games only move when you swap them. You can also fix a single game to its slot.

## Discs, USB sticks and SD cards

A **disc file** is a small JSON file with the `.yasaff` extension that tells YASAFF which game to start:

```json
{
  "yasaff": "disc",
  "version": 1,
  "id": "<YASAFF game id>",
  "ids": { "steam": "1903340", "gog": "2125022825" },
  "name": "Clair Obscur: Expedition 33"
}
```

`id` or at least one entry in `ids` is required. When the `id` matches, that game starts. Otherwise YASAFF looks for the store IDs and lets you choose when more than one game matches.

- **Create disc file…** in a game's options writes the file. **Burn to CD/DVD…** burns it to a disc.
- In **Settings → Physical Devices** you choose what happens **when a disc is inserted** (*Start the game*, *Ask first* or *Do nothing*). You can also check the drive when YASAFF starts and turn on USB stick and SD card support, so those work like discs.
- In a game's **SD and USB Yasaff settings…** you can pick a plugged-in device and **Insert YASAFF launch config into device**. After that, plugging the device in starts the game.

## Saves

- **Set save folder…** tells YASAFF where a game keeps its saves. **Back up saves…** copies them to a timestamped backup.
- **Saves on a launch device:** turn on **Keep saves on the launch device** in **Settings → Physical Devices**, and **Keep saves on device that was used to launch this game** in the game's SD and USB settings. When the game closes, its saves are copied to `YasaffSaves/<game>/` on the USB stick or SD card it was started from. **Disaster Recovery Count** sets how many older timestamped copies are kept. You can track the whole save folder, or only some files and folders.

Each copy is written to a temporary folder first and then renamed, so pulling the device out never leaves a half-written save.

## YASAFF Discover, apps and addons

The **Discover** tile lists apps, games, themes and addons from a YASAFF Discover Server ([YasaffStuffServer](https://github.com/naisyrkdev/YasaffStuffServer)), where you install and update them. Add the store address in **Settings → YASAFF Discover**. You can add several stores and switch between them.

- **Apps** are separate programs that YASAFF starts and holds inside its own window. Press Guide to come back to YASAFF while they keep running.
- **Addons** are bar widgets, for example device info or a clock. Manage them in **Settings → Addons → Addons Manager…** and show each one in the top bar, the bottom bar or anywhere on the screen, at 50–300 % size.

To make your own app, game package or addon, see [docs/YASAFF_application_guide.md](docs/YASAFF_application_guide.md).

## Profiles

Each profile has its own library, settings, themes, images and apps. In **Settings → Profiles** you can switch, rename, create, reset, export (one `.zip`, without store API keys) or import profiles. **Reset application** deletes every profile and all data, like a fresh install.

## Settings overview

| Section | Options |
|---------|---------|
| General | Start in fullscreen, run at system startup, language |
| Game order | Last played games first, static order |
| Profiles | Change, rename, create, export, import, reset |
| Power options | Reset application |
| YASAFF Discover | Active store, address, test connection, add or remove stores |
| Steam / Heroic | Launch games through the store, start the store in the background with YASAFF, custom launch command (auto-detected by default) |
| Physical Devices | When a disc is inserted, check the drive at startup, USB stick and SD card support, keep saves on the launch device |
| Mouse | Virtual mouse on the right stick, speed, custom cursor and cursor size |
| Controller | Button hints, swap A/B (for Nintendo Switch habits), remap every button, Discover and Add game shortcuts, reset bindings |
| Fonts | Game title font, import and manage fonts |
| Theme | Everything in [Customization](#customization) |
| Addons | Addons Manager |
| About | Version, source code, YASAFF Discover Server |

Settings are saved in `userSettings.json` in the profile folder.

## Data folders

| | Linux | Windows |
|-|-------|---------|
| YASAFF data | `~/.local/share/yasaff/` | `%APPDATA%\yasaff\data\` |
| A profile | `…/profiles/<profile>/` | `…\profiles\<profile>\` |

A profile folder holds the library, settings, themes, `apps/`, `appdata/` (data of installed apps, kept on uninstall), `addons/`, and the `apps_installation_packs/` and `addons_installation_packs/` folders that the import dialogs open in.

## Logs

YASAFF writes a log to `<YASAFF data>/logs/yasaff.log` (the previous run's log is kept as `yasaff.old.log` once it grows past 2 MB):

- Linux: `~/.local/share/yasaff/logs/`
- Windows: `%APPDATA%\yasaff\data\logs\`

Attach it when reporting a problem.

## License

See [LICENSE](LICENSE).
