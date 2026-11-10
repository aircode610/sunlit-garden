//! Sunlit Garden's desktop app (Rune). Depends on `sunlit-core`; never the other way round.
//!
//! `cargo run -p sunlit-ui` opens the window; `-- --snap out.png` renders it headless.

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
    let citation = text(s.source.citation()).w_full().wrap().font(type_::BODY).color(t.text_secondary);
    col().w_full().h_full().center().padding(40.0).gap(12.0).fill(t.bg).child(title).child(citation)
}

fn main() {
    rune::run(AppSpec::new("Sunlit Garden", (640.0, 240.0), init, view));
}
