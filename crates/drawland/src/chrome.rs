//! The interface round a screen's map, the same in `drawland view` (a
//! terminal) and `drawland serve` (a page): what its bar, menus, column and
//! status line hold ([`model`]), laid out and drawn by
//! [`drawland_grid::ui`]. The state it shows ([`Ui`]) is the host's.

use drawland_core::paint::ScreenKind;
use drawland_grid::ui::{Button, Line, Menu, Model, THEMES};
use std::path::{Path, PathBuf};

pub const REPO: &str = "https://github.com/piLeoni/drawland";
pub const DOCS: &str = "https://draw.land/docs";

/// What the interface shows and how: the host keeps it, actions change it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ui {
    /// The menu dropped down, by id.
    pub open: Option<String>,
    /// The column on the right is open.
    pub column: bool,
    /// One of [`THEMES`]: on a 1-bit screen `paper` or `ink` (inverted).
    pub theme: String,
    /// The style drawn, by name.
    pub style: String,
    /// A page's size: a text cell's font px, a bitmap pixel's device px.
    pub size: Option<u32>,
    /// A page shows the profile's own screen size, not the window's.
    pub fit_screen: bool,
    /// On a globe, names run on past the rim instead of being cut by it.
    pub names_past_rim: bool,
}

impl Ui {
    pub fn new(style: &str, kind: ScreenKind) -> Ui {
        Ui {
            open: None,
            column: false,
            theme: if kind == ScreenKind::Bitmap { "paper" } else { "ink" }.into(),
            style: style.into(),
            size: None,
            fit_screen: false,
            names_past_rim: false,
        }
    }

    /// Apply a pressed action that is the interface's own (menus, the
    /// column, theme, style, size); `false` for the host's (zoom, edit,
    /// links) or none at all.
    pub fn act(&mut self, action: &str) -> bool {
        let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
        let menu_was_open = self.open.take();
        match verb {
            "open" => self.open = (menu_was_open.as_deref() != Some(arg)).then(|| arg.to_string()),
            "column" => self.column = !self.column,
            "theme" => self.theme = arg.into(),
            "style" => self.style = arg.into(),
            "size" => self.size = arg.parse().ok(),
            "fit" => self.fit_screen = arg == "screen",
            "rim" => self.names_past_rim = arg == "over",
            "none" => {
                self.open = menu_was_open;
                return true;
            }
            _ => return false,
        }
        true
    }

    /// A key's action, `None` for keys the interface leaves to the host.
    pub fn key(&self, key: char) -> Option<String> {
        Some(match key {
            's' => "open:style".into(),
            't' => "open:theme".into(),
            'f' => "open:size".into(),
            'p' | '\\' => "column".into(),
            'n' => format!("rim:{}", if self.names_past_rim { "cut" } else { "over" }),
            _ => return None,
        })
    }
}

/// A screen style: a profile file with an `output.screen`.
#[derive(Clone, Debug)]
pub struct Style {
    pub name: String,
    pub path: PathBuf,
    pub kind: ScreenKind,
}

/// The screen styles beside `path` (its folder's `.js` and `.ts` files
/// whose output is a screen), `path`'s own first; of `kind` when given.
pub fn styles(path: &Path, kind: Option<ScreenKind>) -> Vec<Style> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut found: Vec<Style> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "js" || e == "ts"))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let kind = screen_kind(&text)?;
            Some(Style { name: p.file_stem()?.to_string_lossy().into_owned(), path: p, kind })
        })
        .filter(|s| kind.is_none_or(|k| k == s.kind))
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    let own = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    if let Some(i) = found.iter().position(|s| Some(&s.name) == own.as_ref()) {
        let s = found.remove(i);
        found.insert(0, s);
    }
    found
}

/// The screen a script's text asks for, by a look at its `screen: { kind }`
/// without running it.
fn screen_kind(text: &str) -> Option<ScreenKind> {
    let at = text.find("screen:")?;
    let rest = &text[at..text.len().min(at + 200)];
    if rest.contains("\"text\"") || rest.contains("'text'") {
        Some(ScreenKind::Text)
    } else if rest.contains("\"bitmap\"") || rest.contains("'bitmap'") {
        Some(ScreenKind::Bitmap)
    } else {
        None
    }
}

/// Which host draws the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// `drawland view`: the terminal is the screen; it quits.
    Terminal,
    /// `drawland serve`: a page; its sizes and its window or screen fit.
    Page,
}

/// What the status line and the column say of the map now.
pub struct Now<'a> {
    pub lat: f64,
    pub lng: f64,
    /// Ground a unit spans, mm.
    pub scale: f64,
    pub kind: ScreenKind,
    /// Units across and down.
    pub size: (u32, u32),
    /// The profile's own screen, across and down.
    pub screen: (u32, u32),
    /// The last frame's time, ms, and whether a wider scene is coming.
    pub ms: Option<u64>,
    pub loading: bool,
    /// The last error, shown in the status line.
    pub error: Option<&'a str>,
}

/// The ground a unit spans, from the scale (mm a unit).
pub fn ground(scale: f64, kind: ScreenKind) -> String {
    let unit = match kind {
        ScreenKind::Bitmap => "pixel",
        ScreenKind::Text => "character",
    };
    match scale / 1000.0 {
        m if m >= 1000.0 => format!("{:.1} km a {unit}", m / 1000.0),
        m if m >= 10.0 => format!("{m:.0} m a {unit}"),
        m => format!("{m:.1} m a {unit}"),
    }
}

pub const TEXT_SIZES: &[u32] = &[10, 12, 14, 16, 20, 24];
pub const PIXEL_SIZES: &[u32] = &[1, 2, 3, 4, 6, 8];

/// The interface for `ui`, the styles there are, and the map `now`.
pub fn model(ui: &Ui, styles: &[Style], now: &Now, host: Host) -> Model {
    let bitmap = now.kind == ScreenKind::Bitmap;
    let mut menus = vec![Menu {
        id: "style".into(),
        title: "style".into(),
        key: Some('s'),
        label: format!("{} ▾", ui.style),
        short: format!("{}▾", ui.style),
        options: styles.iter().map(|s| Button::new(&s.name, format!("style:{}", s.name)).on(s.name == ui.style)).collect(),
        right: false,
    }];
    let sizes = if bitmap { PIXEL_SIZES } else { TEXT_SIZES };
    let size_label = |s: u32| if bitmap { format!("×{s}") } else { format!("{s}px") };
    if host == Host::Page {
        let size = ui.size.unwrap_or(if bitmap { 1 } else { 14 });
        menus.push(Menu {
            id: "size".into(),
            title: if bitmap { "pixel" } else { "font" }.into(),
            key: Some('f'),
            label: format!("{} ▾", size_label(size)),
            short: format!("{}▾", size_label(size)),
            options: sizes.iter().map(|&s| Button::new(size_label(s), format!("size:{s}")).on(s == size)).collect(),
            right: false,
        });
    }
    let themes: Vec<&str> = if bitmap { vec!["paper", "ink"] } else { THEMES.iter().map(|(n, _)| *n).collect() };
    menus.push(Menu {
        id: "theme".into(),
        title: "theme".into(),
        key: Some('t'),
        label: format!("{} ▾", ui.theme),
        short: "▾".into(),
        options: themes.iter().map(|t| Button::new(*t, format!("theme:{t}")).on(*t == ui.theme)).collect(),
        right: true,
    });

    let place = format!("{:.5}, {:.5}", now.lat, now.lng);
    let scale = ground(now.scale, now.kind);
    let status = match now.error {
        Some(e) => vec![e.to_string()],
        None => {
            let mut s = vec![place.clone(), scale.clone(), format!("{}×{}", now.size.0, now.size.1)];
            if let Some(ms) = now.ms {
                s.push(format!("{ms} ms"));
            }
            if now.loading {
                s.push("loading round it…".into());
            }
            s
        }
    };
    let mut keys: Vec<(String, String)> = [("s", "style"), ("t", "theme"), ("p", "panel"), ("e", "edit")].iter().map(|(k, d)| (k.to_string(), d.to_string())).collect();
    if host == Host::Terminal {
        keys.push(("q".into(), "quit".into()));
    }

    let column = ui.column.then(|| {
        let mut lines = vec![
            Line::Head("place".into()),
            Line::Text(place.clone()),
            Line::Dim(scale.clone()),
            Line::Blank,
            Line::Row(vec![Button::new(" + ", "zoom:1"), Button::new(" − ", "zoom:-1")]),
            Line::Blank,
            Line::Head("style".into()),
            Line::Text(ui.style.clone()),
        ];
        if let Some(s) = styles.iter().find(|s| s.name == ui.style) {
            lines.push(Line::Dim(s.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()));
        }
        lines.push(Line::Blank);
        lines.push(Line::Row(vec![Button::new("edit", "edit"), Button::new("reload", "reload")]));
        lines.push(Line::Blank);
        lines.push(Line::Head("names on the globe".into()));
        lines.push(Line::Blank);
        lines.push(Line::Row(vec![
            Button::new("cut at rim", "rim:cut").on(!ui.names_past_rim),
            Button::new("past rim", "rim:over").on(ui.names_past_rim),
        ]));
        lines.push(Line::Blank);
        if host == Host::Page {
            let size = ui.size.unwrap_or(if bitmap { 1 } else { 14 });
            lines.push(Line::Head(if bitmap { "pixel" } else { "font" }.into()));
            lines.push(Line::Blank);
            lines.push(Line::Row(sizes.iter().map(|&s| Button::new(size_label(s), format!("size:{s}")).on(s == size)).collect()));
            lines.push(Line::Blank);
            lines.push(Line::Head("screen".into()));
            lines.push(Line::Blank);
            lines.push(Line::Row(vec![
                Button::new("window", "fit:window").on(!ui.fit_screen),
                Button::new(format!("{}×{}", now.screen.0, now.screen.1), "fit:screen").on(ui.fit_screen),
            ]));
            lines.push(Line::Blank);
        }
        lines.push(Line::Head("keys".into()));
        let mut table = vec![("arrows hjkl", "move"), ("+ −", "zoom"), ("s", "style"), ("t", "theme"), ("p", "this panel"), ("n", "names past the rim"), ("e", "edit the style")];
        if host == Host::Terminal {
            table.push(("q", "quit"));
        }
        for (k, d) in table {
            lines.push(Line::Key(k.into(), d.into()));
        }
        lines
    });

    Model {
        brand: "draw.land".into(),
        links: vec![Button::new("map", "none").on(true), Button::new("docs", format!("link:{DOCS}"))],
        menus,
        right: vec![Button::new("GitHub ↗", format!("link:{REPO}"))],
        open: ui.open.clone(),
        column,
        tools: vec![Button::new("+", "zoom:1"), Button::new("−", "zoom:-1"), Button::new("≡", "column")],
        status,
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_open_and_close_menus_and_choose() {
        let mut ui = Ui::new("terminal", ScreenKind::Text);
        assert!(ui.act("open:style"));
        assert_eq!(ui.open.as_deref(), Some("style"));
        assert!(ui.act("open:style"), "pressed again, it closes");
        assert_eq!(ui.open, None);
        ui.act("open:theme");
        assert!(ui.act("theme:ember"));
        assert_eq!((ui.theme.as_str(), ui.open.as_ref()), ("ember", None), "choosing closes the menu");
        ui.act("open:theme");
        assert!(ui.act("none"), "a press on the menu keeps it");
        assert_eq!(ui.open.as_deref(), Some("theme"));
        assert!(!ui.act("zoom:1"), "zoom is the host's");
        assert_eq!(ui.open, None, "and closes the menu");
    }

    #[test]
    fn a_script_says_its_screen_without_running() {
        assert_eq!(screen_kind(r#"output({ screen: { kind: "text", width: 80 } })"#), Some(ScreenKind::Text));
        assert_eq!(screen_kind("output({ screen: { kind: 'bitmap' } })"), Some(ScreenKind::Bitmap));
        assert_eq!(screen_kind("output({ width: 100 })"), None);
    }

    #[test]
    fn the_screen_styles_in_configs_are_found_own_first() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs");
        let all = styles(&root.join("terminal.js"), None);
        assert_eq!(all.first().map(|s| s.name.as_str()), Some("terminal"));
        assert!(all.iter().any(|s| s.name == "1-bit"));
        let bitmaps = styles(&root.join("1-bit.js"), Some(ScreenKind::Bitmap));
        assert!(bitmaps.iter().all(|s| s.kind == ScreenKind::Bitmap) && !bitmaps.is_empty());
    }
}
