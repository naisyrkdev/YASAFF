//! The label, description and value each Settings row shows.

use super::*;

impl App {
    pub(in crate::app) fn setting_row_content(&self, row: SettingRow) -> (String, &'static str, RowValue) {
        use RowValue::{Action, Choice, Toggle};
        let theme = &self.theme().settings;
        let (tiles, grid, bg, boxes) = (&theme.tiles, &theme.grid, &theme.background, &theme.boxes);
        let px = |v: f32| Choice(format!("{v:.0} px"));
        let game = self.selected_game();
        let o = game.map(|g| g.box_style.clone()).unwrap_or_default();
        let has = |image: fn(&Game) -> bool| game.is_some_and(image);
        let game_color = |color: Option<Color32>, theme: Color32| match color {
            Some(c) => RowValue::Color(c),
            None => RowValue::DefaultColor(theme),
        };
        let game_color_sub = tr!("Enter to type a hex value; empty uses the theme's");
        match row {
            SettingRow::Fullscreen => (tr!("Start in fullscreen").into(), "", Toggle(self.settings.start_fullscreen)),
            SettingRow::Autostart => (tr!("Run at system startup").into(), tr!("Start YASAFF when you log in"), Toggle(self.autostart)),
            SettingRow::Language => {
                let label = match self.settings.language {
                    Language::English => "Language".to_owned(),
                    _ => format!("{} · Language", tr!("Language")),
                };
                (label, tr!("Left/Right switches between languages"), Choice(self.settings.language.label().into()))
            }
            SettingRow::OrderLastPlayed => {
                let order = &self.settings.order;
                let sub = if order.static_order {
                    tr!("Off while static order is on")
                } else {
                    tr!("Starting a game moves it to the first slot that isn't static")
                };
                (tr!("Last played games first").into(), sub, Toggle(order.moves_played()))
            }
            SettingRow::OrderStatic => (
                tr!("Static order").into(),
                tr!("Every game stays in its slot; only swapping moves it. Turns off the options above"),
                Toggle(self.settings.order.static_order),
            ),
            SettingRow::AppStoreActive => {
                let stores = &self.settings.app_stores;
                let (sub, value) = match stores.len() {
                    0 | 1 => (tr!("Add another store below to switch between stores"), self.settings.app_store().title()),
                    n => (
                        tr!("Left/Right switches the store the Discover tile shows"),
                        tr!("{store} ({index} of {count})", store = self.settings.app_store().title(), index = self.settings.active_app_store.min(n - 1) + 1, count = n),
                    ),
                };
                (tr!("Store").into(), sub, Choice(value))
            }
            SettingRow::ProfileActive => {
                let n = self.profiles.len();
                let i = self.profiles.iter().position(|p| *p == self.profile_pick).unwrap_or(0);
                let (sub, value) = match (n, self.profile_pick == profiles::active()) {
                    (0 | 1, _) => (tr!("Create another profile below to switch between them"), self.profile_pick.clone()),
                    (_, true) => (tr!("Left/Right picks a profile; Enter switches to it and restarts YASAFF"), tr!("{name} ({index} of {count})", name = self.profile_pick, index = i + 1, count = n)),
                    (_, false) => (tr!("Press Enter to switch; YASAFF restarts"), tr!("{name} ({index} of {count})", name = self.profile_pick, index = i + 1, count = n)),
                };
                (tr!("Change profile").into(), sub, Choice(value))
            }
            SettingRow::ProfileRename => (tr!("Rename current profile").into(), tr!("YASAFF restarts"), Action(tr!("Edit…"))),
            SettingRow::ProfileNew => (tr!("Create new profile").into(), tr!("An empty profile; YASAFF restarts into it"), Action(tr!("Create"))),
            SettingRow::ProfileExport => (tr!("Export current profile…").into(), tr!("Library, settings, themes, images and apps in one .zip (store API keys are left out)"), Action(tr!("Export"))),
            SettingRow::ProfileImport => (tr!("Import profile…").into(), tr!("An exported .zip; added as a new profile, renamed if the name is taken"), Action(tr!("Import"))),
            SettingRow::ProfileReset => (tr!("Reset current profile").into(), tr!("Deletes this profile's library, settings, themes and apps"), Action(tr!("Reset…"))),
            SettingRow::AppReset => (tr!("Reset application").into(), tr!("Deletes every profile and all data, as on a fresh install"), Action(tr!("Reset…"))),
            SettingRow::AppStoreAdd => (tr!("Add another store…").into(), tr!("Each store has its own address"), Action(tr!("Add"))),
            SettingRow::AppStoreRemove => (tr!("Remove this store").into(), tr!("Installed apps and themes stay"), Action(tr!("Remove…"))),
            SettingRow::AboutApp => (
                "YASAFF".into(),
                tr!("A console-style game launcher for PC, made to be used with a controller. Windows and Linux."),
                RowValue::Summary(tr!("Version {version}", version = env!("CARGO_PKG_VERSION"))),
            ),
            SettingRow::AddonManager => (
                tr!("Addons Manager…").into(),
                tr!("Installed addons and importing new ones"),
                RowValue::Summary(tr!("{count} installed", count = self.addons.installed.len())),
            ),
            SettingRow::AboutSource => (tr!("Source code").into(), "github.com/naisyrkdev/YASAFF", Action(tr!("Open"))),
            SettingRow::AboutServer => (
                tr!("YASAFF Discover Server").into(),
                tr!("YasaffStuffServer: the website apps and themes come from"),
                Action(tr!("Open")),
            ),
            SettingRow::VirtualMouse if self.global_mouse.connected() => (
                tr!("Enable virtual mouse on right analog stick").into(),
                tr!("Yasaff Global Virtual Mouse is running and moves the mouse: right stick, RT left-click, LT right-click. Quit it to use this one."),
                RowValue::Summary(tr!("Handled by Global Virtual Mouse").into()),
            ),
            SettingRow::VirtualMouse => (
                tr!("Enable virtual mouse on right analog stick").into(),
                tr!("Right stick moves the cursor; RT or R3 clicks, LT right-clicks"),
                Toggle(self.settings.mouse.virtual_mouse),
            ),
            SettingRow::ControllerPrompts => {
                let value = match self.settings.controls.prompts {
                    Some(family) => family.name().to_owned(),
                    None => tr!("Automatic · {controller}", controller = self.pad_family().name()),
                };
                (tr!("Button hints for").into(), tr!("Automatic follows the controller you use"), Choice(value))
            }
            SettingRow::ControllerSwap => (
                tr!("Swap {a} with {b} action", a = button_token(PadButton::South), b = button_token(PadButton::East)),
                tr!("For those used to Nintendo Switch buttons; hidden while buttons are remapped"),
                Toggle(self.settings.controls.swap_ab),
            ),
            SettingRow::ControllerButton(button) => {
                let control = self.input.bindings().get(button);
                let sub = if self.global_mouse.connected() && GLOBAL_MOUSE_BUTTONS.contains(&button) {
                    tr!("Used by Yasaff Global Virtual Mouse while it runs")
                } else if control == button.default_control() {
                    ""
                } else {
                    tr!("Changed; left/right picks what it does")
                };
                (button_token(button), sub, Choice(control.label().into()))
            }
            SettingRow::SpecialPlace(special) => {
                let place = match self.special_bar(special) {
                    Some(slot) => slot.label(),
                    None => tr!("Library tile"),
                };
                match self.special_button(special) {
                    Some(button) => (
                        format!("{} · {}", special.name(), button_token(button)),
                        tr!("Left/Right picks where it shows: a library tile, or the top or bottom bar"),
                        Choice(place.into()),
                    ),
                    None => (special.name().into(), tr!("Assign it a controller button above to place it in a bar"), Choice(place.into())),
                }
            }
            SettingRow::ControllerReset => (
                tr!("Reset to default bindings").into(),
                tr!("Also brings back the A / B swap option"),
                Action(tr!("Reset")),
            ),
            SettingRow::MouseSpeed => (tr!("Virtual mouse speed").into(), tr!("At full tilt"), Choice(format!("{:.0}", self.settings.mouse.speed))),
            SettingRow::MouseCursor => (
                tr!("Import cursor…").into(),
                tr!("PNG with transparency works best; its top-left corner is the tip"),
                if self.settings.mouse.cursor.is_empty() { Action(tr!("Import")) } else { RowValue::Summary(tr!("Custom").into()) },
            ),
            SettingRow::MouseCursorClear => (tr!("Use the default cursor").into(), tr!("Removes the imported cursor"), Action(tr!("Reset"))),
            SettingRow::MouseCursorSize => (
                tr!("Cursor size").into(),
                tr!("For the imported cursor and the virtual mouse's"),
                Choice(format!("{:.0} px", self.settings.mouse.cursor_size)),
            ),
            SettingRow::FontTitle => {
                let title = &self.settings.fonts.title;
                let value = if title.is_empty() { tr!("Default").to_owned() } else { fonts::display_name(title).to_owned() };
                let sub = if self.fonts.is_empty() { tr!("Import a font below to change it") } else { tr!("Left/Right switches between imported fonts") };
                (tr!("Game title font").into(), sub, Choice(value))
            }
            SettingRow::FontImport => (tr!("Import fonts…").into(), tr!("TrueType or OpenType files (.ttf, .otf, .ttc)"), Action(tr!("Import"))),
            SettingRow::FontManage => (tr!("Manage fonts…").into(), tr!("See the imported fonts and delete them"), RowValue::Summary(tr!("{count} installed", count = self.fonts.len()))),
            SettingRow::GameTitleFont => {
                let default = match self.settings.fonts.title.as_str() {
                    "" => tr!("built-in").to_owned(),
                    title => fonts::display_name(title).to_owned(),
                };
                let value = match game.and_then(|g| g.title_font.as_deref()).filter(|f| self.fonts.iter().any(|l| l == f)) {
                    Some(file) => fonts::display_name(file).to_owned(),
                    None => tr!("Default ({font})", font = default),
                };
                let sub = if self.fonts.is_empty() { tr!("Import fonts in Settings to choose one") } else { tr!("Font of this game's name") };
                (tr!("Title font").into(), sub, Choice(value))
            }
            SettingRow::Theme => {
                let sub = if self.theme().is_builtin() { tr!("Built-in; it can't be deleted") } else { tr!("Left/Right switches between themes") };
                (tr!("Theme").into(), sub, Choice(theme.name.clone()))
            }
            SettingRow::ThemeRename => (tr!("Rename theme").into(), "", Action(tr!("Edit…"))),
            SettingRow::ThemeNew => (tr!("New theme").into(), tr!("Starts as a copy of the current theme"), Action(tr!("Create"))),
            SettingRow::ThemeExport => (tr!("Export theme…").into(), tr!("Copies the theme folder, images included, to a folder you pick"), Action(tr!("Export"))),
            SettingRow::ThemeImport => (tr!("Import theme…").into(), tr!("Select an exported theme folder (with themeSettings.json)"), Action(tr!("Import"))),
            SettingRow::ThemeDelete => (tr!("Delete theme").into(), "", Action(tr!("Delete…"))),
            SettingRow::ViewKind if theme.kind == ThemeKind::Bubbles => {
                (tr!("Layout").into(), tr!("Not available in this pre-defined theme"), Choice(theme.kind.label().into()))
            }
            SettingRow::ViewKind => (
                tr!("Layout").into(),
                tr!("Each layout keeps its own setup; switching back restores it"),
                Choice(theme.view.label().into()),
            ),
            SettingRow::TileStyle => {
                let sub = if theme.kind == ThemeKind::Bubbles { tr!("Not available in this pre-defined theme") } else { tr!("How games are drawn, in either layout") };
                (tr!("Tile style").into(), sub, Choice(theme.tile_style.label().into()))
            }
            SettingRow::BubbleSize => (
                tr!("Bubble size").into(),
                tr!("At 1080p; games that don't fit continue on the next page"),
                px(theme.bubbles.size),
            ),
            SettingRow::BubbleVariance => (
                tr!("Random size difference").into(),
                tr!("Each bubble is up to this much smaller or bigger"),
                Choice(format!("± {:.0} px", theme.bubbles.size_variance)),
            ),
            SettingRow::BubbleFill => (
                tr!("Fill the screen with bubbles").into(),
                tr!("Empty bubbles fill the space around your games"),
                Toggle(theme.bubbles.fill_screen),
            ),
            SettingRow::BubbleShuffle => (tr!("Randomize bubbles").into(), tr!("Picks new random sizes and places"), Action(tr!("Shuffle"))),
            SettingRow::BubbleVideo => (
                tr!("Play video on selected game").into(),
                tr!("Set a video in the game's Options; needs ffmpeg installed"),
                Toggle(theme.bubbles.play_video),
            ),
            SettingRow::BubbleSound => (
                tr!("Sound when swapping bubbles").into(),
                tr!("A soft bloop when moving between bubbles"),
                Toggle(theme.bubbles.sound),
            ),
            SettingRow::BubbleCustomSound => (
                tr!("Use custom sound when swapping bubbles").into(),
                tr!("Plays your own sound file instead of the bloop"),
                Toggle(theme.bubbles.custom_sound),
            ),
            SettingRow::SwapSound => (
                tr!("Sound when swapping game").into(),
                tr!("A soft tick when moving between games"),
                Toggle(theme.swap.sound),
            ),
            SettingRow::SwapCustomSound => (
                tr!("Use custom sound when swapping game").into(),
                tr!("Plays your own sound file instead of the tick"),
                Toggle(theme.swap.custom_sound),
            ),
            SettingRow::BubbleSoundFile | SettingRow::SwapSoundFile => {
                let sub = if self.theme().sound_file(row == SettingRow::BubbleSoundFile).is_some() {
                    tr!("Copied into the theme's folder; WAV, MP3, OGG or FLAC")
                } else {
                    tr!("No sound selected; WAV, MP3, OGG or FLAC")
                };
                (tr!("Custom sound").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::BoxCase => {
                let label = if theme.tile_style == TileStyle::Cover { tr!("Cover shape") } else { tr!("Box shape") };
                (label.into(), tr!("Select box size"), Choice(boxes.case.label().into()))
            }
            SettingRow::BoxWidth | SettingRow::BoxHeight | SettingRow::BoxDepth => {
                let (label, sub, i) = match row {
                    SettingRow::BoxWidth => (tr!("Width"), tr!("50–400 mm; Left/Right steps, Enter to type a value"), 0),
                    SettingRow::BoxHeight => (tr!("Height"), tr!("50–400 mm; Left/Right steps, Enter to type a value"), 1),
                    _ => (tr!("Depth"), tr!("5–100 mm; Left/Right steps, Enter to type a value"), 2),
                };
                (label.into(), sub, Choice(format!("{:.0} mm", boxes.size_mm()[i])))
            }
            SettingRow::BoxSpineLabel => (tr!("Spine label").into(), tr!("On spines without a spine image"), Choice(boxes.spine_label.label().into())),
            SettingRow::BoxSpineColor => (tr!("Spine color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.spine_color)),
            SettingRow::BoxSpineText => (tr!("Spine text color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.spine_text_color)),
            SettingRow::BoxTopColor => (tr!("Top color").into(), tr!("Enter to type a hex value"), RowValue::Color(boxes.top_color)),
            SettingRow::GameCover => {
                let sub = if has(|g| g.asset(AssetKind::Cover).is_some()) { tr!("The front of the box") } else { tr!("None; the name's initials are shown") };
                (tr!("Cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSpine => {
                let sub = if has(|g| g.asset(AssetKind::Spine).is_some()) {
                    tr!("Runs down the spine; a wide image is turned to fit")
                } else {
                    tr!("None; uses the spine color and label")
                };
                (tr!("Spine image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameAdjust(kind) => {
                let p = game.map(|g| g.placement(kind)).unwrap_or_default();
                let value = format!(
                    "{:.0} % · {} · {}",
                    p.zoom * 100.0,
                    position_label(p.x, tr!("Left"), tr!("Right")),
                    position_label(p.y, tr!("Top"), tr!("Bottom"))
                );
                (tr!("Size and position…").into(), tr!("Opens a before / after preview; nothing changes until you accept"), RowValue::Summary(value))
            }
            SettingRow::GameAdjustReset(_) => (tr!("Reset size and position").into(), "", Action(tr!("Reset"))),
            SettingRow::GameSpineClear => (tr!("Remove spine image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameTop => {
                let sub = if has(|g| g.asset(AssetKind::Top).is_some()) { tr!("Lies across the top of the box") } else { tr!("None; uses the top color") };
                (tr!("Top image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameTopClear => (tr!("Remove top image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBack => {
                let sub = if has(|g| g.asset(AssetKind::Back).is_some()) { tr!("Seen while a 3D model box turns") } else { tr!("None; uses the spine color") };
                (tr!("Back cover image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBackClear => (tr!("Remove back cover image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameSide => {
                let sub = if has(|g| g.asset(AssetKind::Side).is_some()) {
                    tr!("The edge opposite the spine; seen while a 3D model box turns")
                } else {
                    tr!("None; uses the top color")
                };
                (tr!("Right side image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameSideClear => (tr!("Remove right side image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBottom => {
                let sub = if has(|g| g.asset(AssetKind::Bottom).is_some()) { tr!("Under the box; seen while editing it") } else { tr!("None; uses the top color") };
                (tr!("Bottom image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBottomClear => (tr!("Remove bottom image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameBooklet => {
                let sub = if has(|g| g.asset(AssetKind::Booklet).is_some()) {
                    tr!("Inside the cover of a 3D opening case")
                } else {
                    tr!("None; the booklet shows the cover")
                };
                (tr!("Instruction booklet image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameBookletClear => (tr!("Remove instruction booklet image").into(), "", Action(tr!("Remove"))),
            SettingRow::GameModel => {
                let sub = if has(|g| g.asset(AssetKind::Model).is_some()) {
                    tr!("STL, OBJ, PLY, glTF / GLB or 3MF; big models are simplified")
                } else {
                    tr!("None; the name is shown in 3D letters")
                };
                (tr!("3D model").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameModelUp => (
                tr!("Model up direction").into(),
                tr!("Models from CAD or 3D printing often need \"Front side up\""),
                Choice(game.map(|g| g.model_up).unwrap_or_default().label().into()),
            ),
            SettingRow::GameModelClear => (tr!("Remove 3D model").into(), tr!("Shows the name in 3D letters again"), Action(tr!("Remove"))),
            SettingRow::GameVideo => {
                let sub = if has(|g| g.asset(AssetKind::Video).is_some()) { tr!("Copied next to the game's images") } else { tr!("None; the cover is shown") };
                (tr!("Preview video").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameVideoClear => (tr!("Remove preview video").into(), "", Action(tr!("Remove"))),
            SettingRow::GameIdleVideo => {
                let sub = if has(|g| g.asset(AssetKind::IdleVideo).is_some()) {
                    tr!("Plays on the CRT screen while the game is not selected")
                } else {
                    tr!("None; the cover is shown")
                };
                (tr!("Idle video").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::GameIdleVideoClear => (tr!("Remove idle video").into(), "", Action(tr!("Remove"))),
            SettingRow::GameObjectColor => (
                tr!("Object color").into(),
                tr!("For the letters and models without colors; empty uses the theme's"),
                game_color(o.object_color, theme.objects.color),
            ),
            SettingRow::DiscKind => (
                tr!("Disc type").into(),
                tr!("All are 120 mm; the shiny data side differs"),
                Choice(theme.discs.kind.label().into()),
            ),
            SettingRow::DiscAngle => {
                let sub = if theme.discs.lying_flat() {
                    tr!("Set by the \"Flat, like in a drive\" spin")
                } else {
                    tr!("How far discs are tipped back; 0° faces you flat")
                };
                (tr!("Angle").into(), sub, Choice(format!("{:.0}°", theme.discs.shown_angle())))
            }
            SettingRow::DiscSpin => (tr!("Spinning").into(), tr!("The selected disc spins"), Toggle(theme.discs.spin)),
            SettingRow::DiscSpinDirection => (
                tr!("Spin").into(),
                tr!("Flipping over also shows the data side; flat lays the disc down"),
                Choice(theme.discs.spin_direction.label().into()),
            ),
            SettingRow::DiscSpinSpeed => (
                tr!("Spin speed").into(),
                tr!("Turns per minute"),
                Choice(format!("{:.0}", theme.discs.spin_speed)),
            ),
            SettingRow::CrtWidth => (tr!("Screen width").into(), tr!("At 1080p; Left/Right steps, Enter to type a value"), px(theme.crt.size()[0])),
            SettingRow::CrtHeight => (tr!("Screen height").into(), tr!("At 1080p; Left/Right steps, Enter to type a value"), px(theme.crt.size()[1])),
            SettingRow::CrtVideo => (
                tr!("Play video on selected game").into(),
                tr!("The game's preview video, on the selected screen"),
                Toggle(theme.crt.play_video),
            ),
            SettingRow::CrtIdleVideo => (
                tr!("Play idle videos").into(),
                tr!("The game's idle video, on the other screens"),
                Toggle(theme.crt.idle_video),
            ),
            SettingRow::CrtFilter => (tr!("CRT filter").into(), tr!("Scanlines, flicker and a darker edge"), Toggle(theme.crt.filter)),
            SettingRow::ObjectColor => (
                tr!("Object color").into(),
                tr!("For the letters and models without colors; Enter to type a hex value"),
                RowValue::Color(theme.objects.color),
            ),
            SettingRow::GameSpineLabel => {
                let value = match o.spine_label {
                    Some(label) => label.label().to_owned(),
                    None => tr!("Default ({value})", value = boxes.spine_label.label()),
                };
                (tr!("Spine label").into(), "", Choice(value))
            }
            SettingRow::GameSpineColor => (tr!("Spine color").into(), game_color_sub, game_color(o.spine_color, boxes.spine_color)),
            SettingRow::GameSpineText => (tr!("Spine text color").into(), game_color_sub, game_color(o.spine_text_color, boxes.spine_text_color)),
            SettingRow::GameTopColor => (tr!("Top color").into(), game_color_sub, game_color(o.top_color, boxes.top_color)),
            SettingRow::TileRows => (
                tr!("Rows").into(),
                tr!("Up to 4; tiles shrink if they don't fit the screen"),
                Choice(tiles.rows.to_string()),
            ),
            SettingRow::TileSize => (tr!("Tile size").into(), tr!("At 1080p; scales with the window"), px(tiles.size)),
            SettingRow::TileSpacing => (tr!("Space between tiles").into(), tr!("At 1080p; scales with the window"), px(tiles.spacing)),
            SettingRow::EnlargeSelected => (tr!("Enlarge the selected tile").into(), "", Toggle(tiles.enlarge_selected)),
            SettingRow::SelectedScale => (
                tr!("Selected tile size").into(),
                tr!("Relative to the other tiles"),
                Choice(format!("{:.0} %", tiles.selected_scale * 100.0)),
            ),
            SettingRow::TileBar => {
                let sub = if tiles.rows > 1 { tr!("Only for a single row; set Rows to 1 to change it") } else { tr!("Only for a single row") };
                (tr!("Tile row position").into(), sub, Choice(tiles.bar_position.label().into()))
            }
            SettingRow::GridColumns => (tr!("Columns").into(), tr!("Tiles grow to fill the page"), Choice(grid.columns.to_string())),
            SettingRow::GridRows => (tr!("Rows per page").into(), tr!("More games continue on the next page"), Choice(grid.rows.to_string())),
            SettingRow::GridSpacing => (tr!("Space between tiles").into(), tr!("At 1080p; scales with the window"), px(grid.spacing)),
            SettingRow::GridEnlarge => (tr!("Enlarge the selected tile").into(), "", Toggle(grid.enlarge_selected)),
            SettingRow::GridAlign => (tr!("Grid position").into(), tr!("When the grid is narrower than the screen"), Choice(grid.align.label().into())),
            SettingRow::GameArt => (
                tr!("Use each game's background art").into(),
                tr!("Changes with the selected game; off keeps the theme background"),
                Toggle(bg.game_art),
            ),
            SettingRow::BackgroundKind => {
                let sub = if bg.game_art { tr!("Shown when the selected game has no background art") } else { tr!("Always shown") };
                (tr!("Background").into(), sub, Choice(bg.kind.label().into()))
            }
            SettingRow::BackgroundColor => {
                let sub = if bg.kind == BackgroundKind::Image { tr!("Around and behind the image") } else { tr!("Enter to type a hex value") };
                (tr!("Color").into(), sub, RowValue::Color(bg.color))
            }
            SettingRow::GradientFrom => (tr!("Start color").into(), tr!("Enter to type a hex value"), RowValue::Color(bg.gradient_from)),
            SettingRow::GradientTo => (tr!("End color").into(), tr!("Enter to type a hex value"), RowValue::Color(bg.gradient_to)),
            SettingRow::GradientDirection => (tr!("Direction").into(), "", Choice(bg.gradient_direction.label().into())),
            SettingRow::BackgroundImage => {
                let sub = if self.theme().background_image().is_some() { tr!("Copied into the theme's images folder") } else { tr!("No image selected") };
                (tr!("Image").into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::ImageFit => (tr!("Image layout").into(), "", Choice(bg.image_fit.label().into())),
            SettingRow::Bar(slot, field) => {
                let bar = theme.bar(slot);
                let percent = |v: f32| Choice(format!("{:+.0} %", v * 100.0));
                match field {
                    BarRow::Enabled => {
                        let sub = match slot {
                            BarSlot::Top => tr!("Behind the library count and the clock"),
                            BarSlot::Bottom => tr!("Behind the button hints"),
                        };
                        (tr!("Show the bar").into(), sub, Toggle(bar.enabled))
                    }
                    BarRow::Height => match self.bar_minimum(slot) {
                        Some((min, addon)) => (
                            tr!("Height · {addon} needs at least {px} px", addon = addon, px = format!("{min:.0}")),
                            tr!("At 1080p; raised to what the addons in this bar need"),
                            px(bar.height(slot).max(min)),
                        ),
                        None => (tr!("Height").into(), tr!("At 1080p; the text inside stays centered"), px(bar.height(slot))),
                    },
                    BarRow::Kind => (tr!("Fill").into(), "", Choice(bar.kind.label().into())),
                    BarRow::Color => {
                        let sub = if bar.kind == BackgroundKind::Image {
                            tr!("Around and behind the image")
                        } else {
                            tr!("Enter to type a hex value; #rrggbbaa makes it see-through")
                        };
                        (tr!("Color").into(), sub, RowValue::Color(bar.color))
                    }
                    BarRow::GradientFrom => (tr!("Start color").into(), tr!("Enter to type a hex value"), RowValue::Color(bar.gradient_from)),
                    BarRow::GradientTo => (tr!("End color").into(), tr!("Enter to type a hex value"), RowValue::Color(bar.gradient_to)),
                    BarRow::GradientDirection => (tr!("Direction").into(), "", Choice(bar.gradient_direction.label().into())),
                    BarRow::Image => {
                        let sub = if self.theme().bar_image(slot).is_some() { tr!("Copied into the theme's images folder") } else { tr!("No image selected") };
                        (tr!("Image").into(), sub, Action(tr!("Choose…")))
                    }
                    BarRow::ImageZoom => (tr!("Image size").into(), tr!("At 100 % the image just covers the bar"), Choice(format!("{:.0} %", bar.image_zoom * 100.0))),
                    BarRow::ImageX => (tr!("Horizontal position").into(), tr!("Moves the image sideways"), percent(bar.image_x)),
                    BarRow::ImageY => (tr!("Vertical position").into(), tr!("Moves the image up and down"), percent(bar.image_y)),
                    BarRow::ImageRotation => (tr!("Rotation").into(), tr!("Turns the image in 5° steps"), Choice(format!("{:.0}°", bar.image_rotation))),
                    BarRow::ImageReset => (tr!("Reset image placement").into(), tr!("Back to 100 %, centered, not rotated"), Action(tr!("Reset"))),
                }
            }
            SettingRow::ButtonStyle => (
                tr!("Button look").into(),
                tr!("Controller images follow the controller; drawn buttons take the colors below"),
                Choice(theme.buttons.style().label().into()),
            ),
            SettingRow::ButtonPick => (
                tr!("Button {button}", button = button_token(self.button_edit)),
                tr!("Left/Right picks the button the rows below change"),
                Choice(self.button_edit.glyph(self.pad_family()).into()),
            ),
            SettingRow::TileIcon(special) => {
                let sub = if self.theme().tile_icon(special.key()).is_some() {
                    tr!("Copied into the theme's images folder")
                } else {
                    tr!("Your own SVG icon, drawn white on the tile")
                };
                (special.name().into(), sub, Action(tr!("Choose…")))
            }
            SettingRow::TileIconClear(special) => {
                (tr!("Remove {tile} icon", tile = special.name()), tr!("Back to the built-in icon"), Action(tr!("Remove")))
            }
            SettingRow::Button(button, field) => {
                let look = theme.buttons.get(button);
                let (_, default_fill, default_text) = drawn_default(self.pad_family(), button);
                let optional = |color: Option<Color32>, default: Color32| match color {
                    Some(c) => RowValue::Color(c),
                    None => RowValue::DefaultColor(default),
                };
                let color_sub = tr!("Enter to type a hex value; empty uses the default");
                match field {
                    ButtonRow::Icon => {
                        let sub = if self.theme().button_icon(button).is_some() {
                            tr!("Copied into the theme's images folder")
                        } else {
                            tr!("Your own image, drawn instead of the button")
                        };
                        (tr!("Image").into(), sub, Action(tr!("Choose…")))
                    }
                    ButtonRow::IconClear => (tr!("Remove image").into(), tr!("Back to the button look above"), Action(tr!("Remove"))),
                    ButtonRow::Shape => (tr!("Shape").into(), "", Choice(look.shape.unwrap_or_default().label().into())),
                    ButtonRow::Color => (tr!("Color").into(), color_sub, optional(look.color, default_fill)),
                    ButtonRow::TextColor => (tr!("Letter color").into(), color_sub, optional(look.text_color, default_text)),
                }
            }
            SettingRow::AppStoreUrl => {
                let url = self.settings.app_store().url.trim();
                let value = if url.is_empty() { tr!("Not set").to_owned() } else { url.to_owned() };
                (tr!("Store address").into(), tr!("The store's site address, e.g. https://store.example.com"), RowValue::Summary(value))
            }
            SettingRow::AppStoreTest => (tr!("Test connection").into(), tr!("The Discover tile lists the store's apps once this works"), Action(tr!("Test"))),
            SettingRow::StoreEnabled(st) => (
                tr!("Launch {store} games through {store}", store = st.short_name()),
                tr!("Its window stays hidden; games start straight from YASAFF"),
                Toggle(self.settings.store(st).enabled),
            ),
            SettingRow::StoreBackground(st) => (
                tr!("Start {store} in the background with YASAFF", store = st.short_name()),
                tr!("Minimized to the tray, below YASAFF, ready for fast launches"),
                Toggle(self.settings.store(st).start_in_background),
            ),
            SettingRow::StoreCommand(st) => {
                let custom = !self.settings.store(st).command.trim().is_empty();
                (tr!("{store} command", store = st.short_name()), if custom { tr!("Custom") } else { tr!("Auto-detected") }, Action(tr!("Edit…")))
            }
            SettingRow::DiscOnInsert => (
                tr!("When a disc is inserted").into(),
                tr!("Asks anyway while YASAFF is in the background"),
                Choice(self.settings.discs.on_insert.label().into()),
            ),
            SettingRow::DiscScanAtStart => (
                tr!("Check the drive when YASAFF starts").into(),
                tr!("A disc left in the drive starts its game, like a console"),
                Toggle(self.settings.discs.scan_at_start),
            ),
            SettingRow::DiscRemovable => (
                tr!("Enable USB drive (USB stick) and SD card support").into(),
                tr!("This option will make those devices behave like a disc"),
                Toggle(self.settings.discs.removable_drives),
            ),
            SettingRow::DiscKeepSaves => (
                tr!("Keep saves on the launch device").into(),
                tr!("Games with it on in Options › SD and USB Yasaff settings copy their saves to the stick they were started from"),
                Toggle(self.settings.discs.keep_saves),
            ),
            SettingRow::DeviceTarget => (
                tr!("Device").into(),
                tr!("Left/Right picks a plugged-in USB stick or SD card"),
                Choice(self.picked_device().map_or_else(|| tr!("None plugged in").to_owned(), volume_name)),
            ),
            SettingRow::DeviceInsertConfig => (
                tr!("Insert YASAFF launch config into device").into(),
                tr!("Writes this game's .yasaff file; plugging the device in then starts the game"),
                Action(tr!("Insert")),
            ),
            SettingRow::DeviceCreateCatalog => (
                tr!("Create a YasaffSaves catalog for this game on device").into(),
                tr!("YasaffSaves/<game> with a backup folder; saves are kept there"),
                if self.picked_catalog().is_some_and(|c| c.is_dir()) { RowValue::Summary(tr!("Created").into()) } else { Action(tr!("Create")) },
            ),
            SettingRow::DeviceKeepSaves => (
                tr!("Keep saves on device that was used to launch this game").into(),
                tr!("When the game closes, its saves are copied to the USB stick or SD card it was started from"),
                Toggle(game.is_some_and(|g| g.device_saves.keep_on_device)),
            ),
            SettingRow::DeviceRecovery => {
                let count = game.map_or(0, |g| g.device_saves.recovery_count);
                let value = if count == 0 { tr!("Off").to_owned() } else { count.to_string() };
                (
                    tr!("Disaster Recovery Count").into(),
                    tr!("Timestamped backups kept in the backup folder, plus the current save"),
                    Choice(value),
                )
            }
            SettingRow::DeviceSavePath(i) => {
                let path = game.and_then(|g| g.device_saves.paths.get(i)).cloned().unwrap_or_default();
                let sub = if path.is_dir() {
                    tr!("Folder, subfolders included · Enter stops tracking it")
                } else if path.exists() {
                    tr!("File · Enter stops tracking it")
                } else {
                    tr!("Not there yet · Enter stops tracking it")
                };
                (path.display().to_string(), sub, Action(tr!("Remove")))
            }
            SettingRow::DeviceSaveFolder => (
                game.and_then(|g| g.save_dir.as_ref()).map(|d| d.display().to_string()).unwrap_or_default(),
                tr!("The save folder from Set save folder…; used until you add files or folders below"),
                RowValue::Summary(tr!("Save folder").into()),
            ),
            SettingRow::DeviceAddFiles => (tr!("Add save files…").into(), tr!("One or more files the game saves to"), Action(tr!("Add"))),
            SettingRow::DeviceAddFolders => (
                tr!("Add save folders…").into(),
                tr!("Each folder is copied with everything in it, subfolders too"),
                Action(tr!("Add")),
            ),
        }
    }
}
