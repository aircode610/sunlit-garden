//! Sunlit Garden's desktop app (Rune). Depends on `sunlit-core`; never the other way round.
//!
//! `cargo run -p sunlit-ui` opens the window; `-- --snap out.png` renders it headless,
//! `-- --snap out.png --scene hume` renders a named scene (`-- --list-scenes` lists them).

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, col, text};
use sunlit_core::Source;

/// Everything the window shows. Rune redraws by calling `view` with the current state.
struct State {
    source: Source,
}

fn init() -> State {
    State {
        source: Source {
            thinker: "George Berkeley".into(),
            work: "A Treatise Concerning the Principles of Human Knowledge".into(),
            edition: "Project Gutenberg #4723".into(),
            url: "https://www.gutenberg.org/ebooks/4723".into(),
        },
    }
}

/// State in, element tree out. No drawing code: Rune lays out and renders the tree.
fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true); // `true` = dark theme
    let title = text("Sunlit Garden").font(type_::TITLE).color(t.text);
    // `w_full().wrap()`: take the column's width and break long lines instead of overflowing.
    let citation = text(s.source.citation())
        .w_full()
        .wrap()
        .font(type_::BODY)
        .color(t.text_secondary);
    col()
        .w_full()
        .h_full()
        .center()
        .padding(40.0)
        .gap(12.0)
        .fill(t.bg)
        .child(title)
        .child(citation)
}

/// Scene "hume": same window, a different source. Scenes are named states for headless renders.
fn hume(s: &mut State) {
    s.source = Source {
        thinker: "David Hume".into(),
        work: "Dialogues Concerning Natural Religion".into(),
        edition: "Project Gutenberg #4583".into(),
        url: "https://www.gutenberg.org/ebooks/4583".into(),
    };
}

fn main() {
    rune::run(AppSpec {
        scenes: &[("hume", hume)],
        // `..` fills every other field from `AppSpec::new` (struct update syntax).
        ..AppSpec::new("Sunlit Garden", (640.0, 240.0), init, view)
    });
}
