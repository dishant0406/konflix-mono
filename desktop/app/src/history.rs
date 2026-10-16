//! The screens the viewer has been through, and what Back does.
//!
//! It is a stack, with rules that keep Back from wandering:
//! - Home and the catalogue tabs are starting points, not steps: opening one clears what came
//!   before, so Back from a tab goes Home, and switching tabs back and forth never builds a trail.
//! - Opening a screen that is already in the stack goes back to it instead of stacking a second
//!   copy, so Back can never loop between the same screens.
//! - Refining a search replaces the search it follows, so Back leaves the search.

use crate::route::Route;

/// The most screens kept. Older ones are forgotten, the way a browser forgets.
const LONGEST: usize = 30;

pub struct History<T> {
    entries: Vec<(Route, T)>,
}

impl<T: Default> History<T> {
    pub fn new(first: Route, data: T) -> Self {
        History {
            entries: vec![(first, data)],
        }
    }

    pub fn route(&self) -> &Route {
        &self.entries.last().expect("history is never empty").0
    }

    pub fn current(&self) -> &T {
        &self.entries.last().expect("history is never empty").1
    }

    pub fn current_mut(&mut self) -> &mut T {
        &mut self.entries.last_mut().expect("history is never empty").1
    }

    /// Whether Back leads anywhere. With nothing behind (the app was opened straight onto a
    /// video), it leads Home.
    pub fn can_go_back(&self) -> bool {
        self.entries.len() > 1 || self.route() != &Route::Home
    }

    /// Opens a screen. The caller has checked it is not the one already showing.
    pub fn go(&mut self, route: Route, data: T) {
        if route == Route::Home {
            self.entries.clear();
        } else if route.is_start() {
            self.entries.clear();
            self.entries.push((Route::Home, T::default()));
        } else if route.is_search() && self.route().is_search() {
            self.entries.pop();
        } else if let Some(earlier) = self.entries.iter().position(|(seen, _)| seen == &route) {
            self.entries.truncate(earlier);
        }
        self.entries.push((route, data));
        if self.entries.len() > LONGEST {
            self.entries.remove(0);
        }
    }

    /// Swaps the screen without adding a step, so Back skips it. Stepping from one episode to
    /// the next uses this: Back should leave the show, not walk through every episode.
    pub fn replace(&mut self, route: Route, data: T) {
        *self.entries.last_mut().expect("history is never empty") = (route, data);
    }

    /// One step back. Returns whether the screen changed.
    pub fn back(&mut self) -> bool {
        if self.entries.len() > 1 {
            self.entries.pop();
            true
        } else if self.route() != &Route::Home {
            self.replace(Route::Home, T::default());
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akidev_core::model::Kind;

    fn tab(kind: Kind) -> Route {
        Route::Browse {
            kind: Some(kind),
            term: String::new(),
        }
    }

    fn search(term: &str) -> Route {
        Route::Browse {
            kind: None,
            term: term.into(),
        }
    }

    fn title(id: &str) -> Route {
        Route::Title { id: id.into() }
    }

    fn watch(id: &str) -> Route {
        Route::Watch { id: id.into() }
    }

    fn history() -> History<()> {
        History::new(Route::Home, ())
    }

    fn visit(history: &mut History<()>, routes: &[Route]) {
        for route in routes {
            if history.route() != route {
                history.go(route.clone(), ());
            }
        }
    }

    /// Presses Back until it stops, and lists the screens it passed through.
    fn back_out(history: &mut History<()>) -> Vec<Route> {
        let mut seen = Vec::new();
        while history.back() {
            seen.push(history.route().clone());
        }
        seen
    }

    #[test]
    fn switching_tabs_leaves_no_trail() {
        let mut history = history();
        visit(
            &mut history,
            &[
                tab(Kind::Anime),
                tab(Kind::Series),
                Route::Home,
                tab(Kind::Anime),
                tab(Kind::Movie),
            ],
        );
        assert_eq!(history.route(), &tab(Kind::Movie));
        assert_eq!(back_out(&mut history), [Route::Home]);
        assert!(!history.can_go_back());
    }

    #[test]
    fn back_walks_the_way_in_and_ends_at_home() {
        let mut history = history();
        visit(
            &mut history,
            &[tab(Kind::Series), title("series-1"), watch("series-1-s1e1")],
        );
        assert_eq!(
            back_out(&mut history),
            [title("series-1"), tab(Kind::Series), Route::Home]
        );
    }

    #[test]
    fn opening_a_screen_already_in_the_stack_goes_back_to_it() {
        let mut history = history();
        visit(
            &mut history,
            &[title("a"), watch("a-s1e1"), search("a"), title("a")],
        );
        assert_eq!(history.route(), &title("a"));
        assert_eq!(back_out(&mut history), [Route::Home]);
    }

    #[test]
    fn refining_a_search_replaces_it() {
        let mut history = history();
        visit(
            &mut history,
            &[search("nar"), search("naru"), search("naruto")],
        );
        assert_eq!(back_out(&mut history), [Route::Home]);
    }

    #[test]
    fn a_search_after_a_title_stacks_on_it() {
        let mut history = history();
        visit(&mut history, &[search("x"), title("a"), search("y")]);
        assert_eq!(
            back_out(&mut history),
            [title("a"), search("x"), Route::Home]
        );
    }

    #[test]
    fn settings_is_a_step_so_back_returns_to_where_the_viewer_was() {
        let mut history = history();
        visit(&mut history, &[title("a"), Route::Settings]);
        assert_eq!(back_out(&mut history), [title("a"), Route::Home]);
    }

    #[test]
    fn replacing_leaves_no_step_behind() {
        let mut history = history();
        visit(&mut history, &[title("a"), watch("a-s1e1")]);
        history.replace(watch("a-s1e2"), ());
        history.replace(watch("a-s1e3"), ());
        assert_eq!(back_out(&mut history), [title("a"), Route::Home]);
    }

    #[test]
    fn opened_on_a_video_back_goes_home_and_then_stops() {
        let mut history = History::new(watch("movie-1"), ());
        assert!(history.can_go_back());
        assert!(history.back());
        assert_eq!(history.route(), &Route::Home);
        assert!(!history.can_go_back());
        assert!(!history.back());
    }

    #[test]
    fn only_the_most_recent_screens_are_kept() {
        let mut history = history();
        for n in 0..(LONGEST * 2) {
            history.go(title(&format!("t{n}")), ());
        }
        assert_eq!(history.entries.len(), LONGEST);
        assert_eq!(history.route(), &title(&format!("t{}", LONGEST * 2 - 1)));
    }
}
