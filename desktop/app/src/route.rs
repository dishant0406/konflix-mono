//! Where the app is, and how to get somewhere else.

use akidev_core::media_id;
use akidev_core::model::Kind;
use gpui_kit::{App, Global, WeakEntity};

#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Home,
    /// Browsing a catalogue (all of them when `kind` is `None`), or the results of a search
    /// when `term` is not empty.
    Browse {
        kind: Option<Kind>,
        term: String,
    },
    /// A title's detail page. Takes a title key such as `series-tt0903747`.
    Title {
        id: String,
    },
    /// A watch page. Takes an episode or film id such as `series-tt0903747-s1e5`.
    Watch {
        id: String,
    },
    /// The music tab: the home shelves, or the results of a search when `term` is not empty.
    Music {
        term: String,
    },
    MusicAlbum {
        id: String,
    },
    MusicArtist {
        id: String,
    },
    Settings,
}

impl Route {
    /// The screen to open at launch: `akidev`, `akidev settings`, `akidev anime` (or
    /// `series`, `movie`), `akidev search <words>`, `akidev music [words]`, `akidev music-album <id>`,
    /// `akidev music-artist <id>` or `akidev <media id>`.
    pub fn from_args(args: impl Iterator<Item = String>) -> Route {
        let args: Vec<String> = args.collect();
        match args.first().map(String::as_str) {
            None | Some("home") => Route::Home,
            Some("settings") => Route::Settings,
            Some("music-album") if args.len() > 1 => Route::MusicAlbum {
                id: args[1].clone(),
            },
            Some("music-artist") if args.len() > 1 => Route::MusicArtist {
                id: args[1].clone(),
            },
            Some("music") => Route::Music {
                term: args[1..].join(" "),
            },
            Some("search") => Route::Browse {
                kind: None,
                term: args[1..].join(" "),
            },
            Some(name) if Kind::parse(name).is_some() && args.len() == 1 => Route::Browse {
                kind: Kind::parse(name),
                term: String::new(),
            },
            Some(id) => Route::for_media_id(id),
        }
    }

    /// A place to start from rather than a step on the way: Home and the catalogue tabs.
    pub fn is_start(&self) -> bool {
        matches!(self, Route::Home)
            || matches!(self, Route::Browse { term, .. } if term.is_empty())
            || matches!(self, Route::Music { term } if term.is_empty())
    }

    pub fn is_search(&self) -> bool {
        matches!(self, Route::Browse { term, .. } if !term.is_empty())
    }

    /// The route a media id belongs to: an episode or film plays, a title key is a detail page.
    pub fn for_media_id(id: &str) -> Route {
        match media_id::parse(id) {
            Some(parsed) if parsed.episode.is_some() || parsed.kind == Kind::Movie => {
                Route::Watch { id: id.to_string() }
            }
            _ => Route::Title { id: id.to_string() },
        }
    }
}

/// The root view, so any view can navigate without holding a reference to its parent.
pub struct Navigator(pub WeakEntity<crate::app::AkidevApp>);

impl Global for Navigator {}

pub fn navigate(route: Route, cx: &mut App) {
    let Some(navigator) = cx.try_global::<Navigator>() else {
        return;
    };
    let app = navigator.0.clone();
    let _ = app.update(cx, |app, cx| app.go(route, cx));
}

/// Shows a short message at the bottom of the window.
pub fn toast(text: impl Into<String>, cx: &mut App) {
    let Some(navigator) = cx.try_global::<Navigator>() else {
        return;
    };
    let app = navigator.0.clone();
    let text = text.into();
    let _ = app.update(cx, |app, cx| app.toast(text, cx));
}

/// Moves to another screen without adding to the history, so Back skips it. Stepping from
/// one episode to the next uses this: Back should leave the show, not walk back through
/// every episode.
pub fn replace(route: Route, cx: &mut App) {
    let Some(navigator) = cx.try_global::<Navigator>() else {
        return;
    };
    let app = navigator.0.clone();
    let _ = app.update(cx, |app, cx| app.replace(route, cx));
}

/// Goes back one screen, as the back button does.
pub fn go_back(cx: &mut App) {
    let Some(navigator) = cx.try_global::<Navigator>() else {
        return;
    };
    let app = navigator.0.clone();
    let _ = app.update(cx, |app, cx| app.back(cx));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(args: &[&str]) -> Route {
        Route::from_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn launch_arguments_pick_the_first_screen() {
        assert_eq!(route(&[]), Route::Home);
        assert_eq!(route(&["settings"]), Route::Settings);
        assert_eq!(
            route(&["anime"]),
            Route::Browse {
                kind: Some(Kind::Anime),
                term: String::new()
            }
        );
        assert_eq!(
            route(&["search", "bad", "wolf"]),
            Route::Browse {
                kind: None,
                term: "bad wolf".into()
            }
        );
        assert_eq!(
            route(&["series-tt1"]),
            Route::Title {
                id: "series-tt1".into()
            }
        );
        assert_eq!(
            route(&["series-tt1-s1e2"]),
            Route::Watch {
                id: "series-tt1-s1e2".into()
            }
        );
        assert_eq!(
            route(&["movie-tt9"]),
            Route::Watch {
                id: "movie-tt9".into()
            }
        );
    }
}
