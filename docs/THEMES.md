# Making a custom theme

A custom theme draws YASAFF's home screen with its own code. It decides the layout, how the controller moves the selection, and the animation. It also declares its own options, pictures and a per-game picture, and YASAFF shows those in Settings like its own.

[YasaffDoubleAxisTheme](../../YasaffDoubleAxisTheme/) is a complete one: categories along the top, their games below. Copy it to start a new theme.

## The package

```
my_theme.zip
├── themeSettings.json   # "kind": "custom", plus the look YASAFF draws around the theme
├── theme.wasm           # the code
└── images/              # pictures the code draws, backgrounds, bar images, sounds
```

The files can also sit inside a single top-level folder. Install a package with **Settings → Theme → Import theme…**, or publish it on a Discover server. **Export theme…** writes the same package back out, with `theme.wasm` included.

`themeSettings.json` is an ordinary theme file. YASAFF still draws the background, the bars, the button hints and the widgets, and plays the swap sound, so those settings apply to a custom theme too. A custom theme only needs:

```json
{ "name": "My theme", "kind": "custom" }
```

The values the user picks for the theme's options are saved in that file under `custom.values`. The pictures the user picks go in `custom.images` and `custom.placements`. A package can ship defaults for both.

## The code

`theme.wasm` is a WebAssembly module, run sandboxed in an interpreter (wasmi). It can't import anything: no files, network, clock or WASI. It only sees the JSON YASAFF passes in. Each call is limited in how long it can run and how much memory it can use. When the code fails, YASAFF shows the error and falls back to the row view.

In Rust, build the theme as a `cdylib` for `wasm32-unknown-unknown`. Other languages work as long as the module exports these four:

| Export | Called | Does |
|--------|--------|------|
| `memory` | | The module's memory |
| `yasaff_alloc(len: i32) -> i32` | before each call | Returns room for `len` bytes. YASAFF writes the call's JSON input there. |
| `yasaff_describe(ptr: i32, len: i32) -> i64` | when the theme loads, or the language changes | Takes `{"language": …}` and returns the manifest |
| `yasaff_frame(ptr: i32, len: i32) -> i64` | every frame the home screen shows | Takes the frame and returns what to draw |

An `i64` result points at the theme's JSON answer in its memory: the address in the upper 32 bits and the length in the lower 32.

The module stays loaded while the theme is selected, so it can keep state between frames, such as which column is open or where an animation is. Changing `theme.wasm` on disk reloads it.

## The manifest

`yasaff_describe` returns the options to show in **Settings → Home screen**, and the game picture. Write the labels in the `language` it was given (`english`, `french`, `spanish` or `polish`).

```json
{
  "options": [
    { "key": "width", "type": "number", "label": "Box width", "description": "At 1080p",
      "section": "My theme", "section_description": "Shown above the options",
      "min": 80, "max": 600, "step": 10, "default": 220, "unit": "px" },
    { "key": "glow", "type": "number", "label": "Glow", "min": 0, "max": 1, "step": 0.1, "default": 0.6,
      "unit": "%", "display_scale": 100, "zero_label": "Off" },
    { "key": "border", "type": "toggle", "label": "Border", "default": false },
    { "key": "size", "type": "number", "label": "Font size", "min": 10, "max": 80, "step": 2, "default": 28, "disabled_by": "auto_size" },
    { "key": "color", "type": "color", "label": "Text color", "default": "#ffffff" },
    { "key": "align", "type": "choice", "label": "Alignment", "default": "left",
      "choices": [{ "value": "left", "label": "Left" }, { "value": "right", "label": "Right" }] },
    { "key": "font", "type": "font", "label": "Font" },
    { "key": "images", "type": "pictures", "label": "Pictures…", "categories": true, "size": ["width", 120],
      "empty": "None. The name is shown",
      "slots": [{ "key": "logo", "label": "Logo", "section": "Other", "size": [200, 200] }] }
  ],
  "game_picture": {
    "label": "List image", "section": "My theme", "size": [104, 104], "seed_from_cover": true
  }
}
```

| Option `type` | Value the theme gets | In Settings |
|---------------|----------------------|-------------|
| `number` | a number from `min` to `max` | Left/Right moves by `step`, and Enter lets the user type a value. `display_scale`, `unit` and `zero_label` only change how the value is shown. |
| `toggle` | `true` or `false` | |
| `color` | `"#rrggbb"` or `"#rrggbbaa"` | Left/Right cycles presets, and Enter lets the user type a hex value |
| `choice` | one of the `value`s | |
| `font` | an imported font's file name, or `null` for the game title font | |
| `pictures` | none; the theme gets the picture keys in `library.pictures` | Opens a page of pictures: one for each category (keyed `category:<name>`, when `categories` is set), then each of `slots` |

`section` starts a new heading at that option or slot. `disabled_by` names a toggle that turns the option off while it is on. A `size` is two numbers, or keys of `number` options, giving the box a picture is shown in, so the size-and-position editor previews it at the right shape.

`game_picture` adds a picture to each game's **Options → Appearance**. With `seed_from_cover`, each game starts with a copy of its cover the first time the theme is used.

## A frame

YASAFF calls `yasaff_frame` with this input:

```json
{
  "screen": [0, 0, 1920, 1080],
  "area": [0, 88, 1920, 892],
  "scale": 1.0,
  "time": 12.5, "dt": 0.016,
  "interactive": true,
  "selected": 3,
  "library": {
    "language": "english",
    "categories": ["Racing"],
    "pictures": ["category:Racing"],
    "tiles": [
      { "kind": "game", "name": "Omaro", "status": "Last played today", "categories": ["Racing"],
        "main_category": "Racing", "picture": true, "running": false },
      { "kind": "discover", "name": "Discover", "status": "Apps, games and themes…", "icon": true }
    ]
  },
  "options": { "width": 220, "glow": 0.6, "border": false, "color": "#ffffff", "align": "left", "font": null },
  "events": [{ "nav": "down" }, { "click": 7 }]
}
```

- `screen` and `area` are `[x, y, width, height]` in screen points. `area` is the space between the top and bottom bars. Multiply sizes given at 1080p by `scale`.
- `library` and `options` are sent only on the first frame and when they change. Keep the last ones you got.
- `tiles` lists the games, then the built-in tiles (`discover`, `add` and `settings`) that aren't moved to a bar. `selected` and every other tile index point into this list.
- `events` holds the directions pressed and the clicked `hits` from the last frame. Confirm, Options and Back are handled by YASAFF: Confirm starts the selected tile.
- `interactive` is off while a menu or Settings covers the home screen.

The theme answers with:

```json
{
  "draw": [
    { "type": "rect", "rect": [100, 100, 220, 120], "radius": 8, "fill": "#00000080", "stroke": { "width": 2, "color": "#ffffff" } },
    { "type": "glow", "rect": [100, 100, 220, 120], "radius": 8, "color": "#8fc8ff", "strength": 0.6 },
    { "type": "circle", "center": [90, 160], "radius": 6, "fill": "#5cb83c" },
    { "type": "image", "source": { "tile": 0 }, "rect": [100, 260, 104, 104], "alpha": 1, "radius": 6,
      "glow": { "color": "#8fc8ff", "strength": 0.6 } },
    { "type": "text", "text": "Omaro", "pos": [224, 300], "align": ["left", "center"], "size": 40,
      "font": { "tile": 0 }, "color": "#ffffff", "shadow": true },
    { "type": "fit_text", "text": "Racing", "rect": [110, 110, 200, 100], "max_size": 28, "font": "title", "color": "#ffffff" }
  ],
  "hits": [{ "rect": [100, 100, 220, 120], "id": 7 }],
  "select": 0,
  "click": 0
}
```

Shapes are drawn in the order given, over the background and under menus.

| Field | Meaning |
|-------|---------|
| image `source` | `{"tile": i}` a game's picture, `{"icon": i}` a built-in tile's icon, `{"picture": key}` one of the theme's pictures, `{"file": name}` a file in the theme's `images/` |
| image `glow` | A halo that follows the picture's outline (transparent parts don't glow) |
| text `align` | Horizontal `left`/`center`/`right`, then vertical `top`/`center`/`bottom`, around `pos` |
| text `glow` / `shadow` | A halo around the letters; without one, `shadow` (on by default) adds a drop shadow |
| `font` | `"title"` (the game title font), `"body"`, `{"file": name}` (an imported font, e.g. a `font` option's value), `{"tile": i}` (a game's own title font) |
| `fit_text` | Centered and justified in `rect`, at the biggest size up to `max_size` that fits without breaking a word |
| `hits` | Areas whose clicks come back next frame as `{"click": id}` |
| `select` | Selects that tile. YASAFF plays the swap sound. |
| `click` | Acts as a click on that tile: selects it, or starts it when it's already selected |

## Building and checking it

YasaffDoubleAxisTheme's `package.sh` builds the module and writes the package:

```sh
rustup target add wasm32-unknown-unknown
./package.sh          # dist/<name>.zip
```

`src/yasaff.rs` in that repository holds the protocol types and the four exports. Copy it unchanged, implement its `Theme` trait, and call `export_theme!(YourTheme)`. Theme logic is ordinary Rust, so `cargo test` can drive `frame` with JSON input, as that theme's tests do.
