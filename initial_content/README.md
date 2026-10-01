# Initial content

The games every new YASAFF profile starts with. Themes aren't bundled: custom themes, such as [YasaffDoubleAxisTheme](../../YasaffDoubleAxisTheme/), are installed from their packages or from Discover.

## Games

They are mocked games: launching one only shows its logo and the note that it can be deleted.

### How they get into YASAFF

1. `build.py` packs each folder in `games/` into `dist/<id>.zip`.
2. YASAFF embeds those zips at compile time (`src/initial.rs`).
3. When a profile starts with an empty library and hasn't been set up yet, YASAFF writes the zips to `<YASAFF data>/initial_games/` and installs **every** `.zip` in that folder in the background, showing "Setting up initial stuff…". On Linux that folder is `~/.local/share/yasaff/initial_games/`.
4. The profile then gets an `initialContent.json` marker. Deleting the games doesn't bring them back. A new or reset profile gets them again.

### A game folder

| File | Goes to |
|------|---------|
| `yasaffConfig.json` | the manifest (`"builtin": "mock-game"`, so no program is shipped) |
| `game_logo.png` | shown by the mock game window (`artwork.logo`) |
| `square_cover.png` | the game's cover: tiles, bubbles and the box front (`icon`) |
| `standard_cover.png` | the portrait front of the 3D case (`artwork.booklet`) |
| `3d_box_spine.png` | the box spine (`artwork.spine`) |
| `3d_top_image.png` | the box top (`artwork.top`) |
| `3d_bottom_image.png` | the box bottom (`artwork.bottom`) |
| `3d_right_image.png` | the box right side (`artwork.side`) |

### Changing the images

The current images are placeholders: the game's name on a solid color. To replace one, drop a PNG with the same name into the game folder, then:

```sh
./build.py            # repacks dist/*.zip; makes placeholders only for missing images
cargo build           # YASAFF embeds the new zips
```

`./build.py --placeholders` regenerates every placeholder and overwrites real images. Needs Pillow (`pip install pillow`).

To add a game, add a folder with a `yasaffConfig.json` like the others, run `build.py`, and add its zip to `BUNDLED` in `src/initial.rs`.
