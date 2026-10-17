//! The watch page's side panels: the frame they share, the source picker and the subtitle menu.
//! They only draw what they are given and report a click as an action; the page decides.

use crate::captions::{self, Group};
use crate::choices;
use crate::design::state::{empty_state, error_state, skeleton};
use crate::design::tokens::{
    ACCENT, LINE, SURFACE, TEXT, TEXT_DIM, TEXT_FAINT, TOP_BAR, WASH, WASH_2, color, radius,
};
use crate::player::SubtitleTrack;
use akidev_core::stream::Choice;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

/// The panel on the right, under the top bar: a title with a close button over the body.
/// The body decides for itself what scrolls.
pub fn shell(
    title: &str,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    body: impl IntoElement,
) -> Div {
    div()
        .occlude()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(400.))
        .flex()
        .flex_col()
        .bg(color(SURFACE).opacity(0.98))
        .border_l_1()
        .border_color(color(LINE))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                // As tall as the top bar, so the title sits level with the bar's own text.
                .h(px(TOP_BAR))
                .pl_5()
                .pr_3()
                .border_b_1()
                .border_color(color(LINE))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(TEXT))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .id("close-panel")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(28.))
                        .rounded(px(radius::BUTTON))
                        .cursor_pointer()
                        .hover(|style| style.bg(color(TEXT).opacity(WASH_2)))
                        .on_click(on_close)
                        .child(Icon::new(IconName::X).small().text_color(color(TEXT_DIM))),
                ),
        )
        .child(div().flex_1().min_h_0().flex().flex_col().child(body))
}

fn scroll_area(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .restrict_scroll_to_axis()
}

/// A quiet small-caps line that names a group of rows.
fn section_label(text: &str) -> Div {
    div()
        .px_4()
        .pt_3()
        .pb_1()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(TEXT_FAINT))
        .child(crate::design::state::spaced(text))
}

/// A clickable row: highlighted when it is the one in use.
fn row(id: impl Into<ElementId>, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_2()
        .py_2p5()
        .rounded(px(radius::PANEL))
        .cursor_pointer()
        .when(selected, |row| row.bg(color(TEXT).opacity(WASH)))
        .hover(|style| style.bg(color(TEXT).opacity(WASH)))
}

/// A fixed-width slot that holds the check mark, so rows line up whether marked or not.
fn mark(selected: bool) -> Div {
    div()
        .flex_none()
        .w(px(16.))
        .children(selected.then(|| Icon::new(IconName::Check).small().text_color(color(ACCENT))))
}

fn badge(text: &str, accent: bool) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .rounded(px(radius::BADGE))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(accent, |badge| {
            badge
                .bg(color(ACCENT).opacity(0.14))
                .text_color(color(ACCENT))
        })
        .when(!accent, |badge| {
            badge
                .bg(color(TEXT).opacity(WASH))
                .text_color(color(TEXT_DIM))
        })
        .child(text.to_string())
}

// ---------------------------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------------------------

/// What a panel calls when a row is clicked.
pub type Act<A> = Rc<dyn Fn(A, &mut App)>;

pub enum SourcesView<'a> {
    Loading,
    Failed(&'a str),
    Ready(&'a [Choice]),
}

pub enum SourceAction {
    Pick(String),
    Retry,
}

/// Every release found, the usual formats first and the others under their own divider.
/// `playing` is the info hash of the one on screen.
pub fn sources_body(view: SourcesView, playing: Option<&str>, act: Act<SourceAction>) -> Div {
    let list = scroll_area("sources-scroll");
    let body = match view {
        SourcesView::Loading => list.child(
            div().flex().flex_col().gap_1().p_2().children(
                [260., 300., 220., 280., 240., 300.]
                    .into_iter()
                    .map(|width| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .px_2()
                            .py_2p5()
                            .child(skeleton(px(width), px(13.)))
                            .child(skeleton(px(150.), px(11.)))
                    }),
            ),
        ),
        SourcesView::Failed(reason) => list.child(error_state(
            "Couldn't list the sources",
            reason.to_string(),
            move |_, _, cx| act(SourceAction::Retry, cx),
        )),
        SourcesView::Ready([]) => list.child(empty_state(
            "No sources yet",
            "Nothing is listed for this one. Check back later; new releases appear as they are shared.",
        )),
        SourcesView::Ready(all) => {
            let (usual, other) = choices::split(all);
            let source_row = |choice: &Choice| {
                let id = choice.id.clone();
                let is_playing = playing.is_some_and(|hash| hash.eq_ignore_ascii_case(&choice.id));
                let act = act.clone();
                row(
                    SharedString::from(format!("source-{}", choice.id)),
                    is_playing,
                )
                .items_start()
                .on_click(move |_, _, cx| act(SourceAction::Pick(id.clone()), cx))
                .px_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .line_height(px(18.))
                                .text_color(color(TEXT))
                                .line_clamp(2)
                                .child(choice.title.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .text_color(color(TEXT_DIM))
                                .children(is_playing.then(|| {
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(color(ACCENT))
                                        .child(crate::design::state::spaced("Playing"))
                                }))
                                .child(choices::badges(choice).join(" · ")),
                        ),
                )
            };
            list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .children(usual.into_iter().map(source_row))
                    .when(!other.is_empty(), |list| {
                        list.child(
                            div()
                                .mt_2()
                                .border_t_1()
                                .border_color(color(LINE))
                                .child(section_label("Other formats").px_2()),
                        )
                        .children(other.into_iter().map(source_row))
                    }),
            )
        }
    };
    div().flex_1().min_h_0().flex().flex_col().child(body)
}

// ---------------------------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------------------------

pub struct SubtitleMenu<'a> {
    /// The track on screen, if any.
    pub selected: Option<i64>,
    /// The tracks inside the file.
    pub embedded: Vec<&'a SubtitleTrack>,
    pub online: &'a [Group],
    /// The online subtitle in use, by id, when one is.
    pub online_in_use: Option<&'a str>,
    /// The language whose options are showing.
    pub expanded: Option<&'a str>,
}

pub enum SubtitleAction {
    Off,
    Embedded(i64),
    Online(String),
    /// Opens or closes a language's options.
    Language(String),
}

/// Off, the file's own tracks, then the online ones one language at a time.
pub fn subtitles_body(menu: SubtitleMenu, act: Act<SubtitleAction>) -> Div {
    let off = {
        let act = act.clone();
        row("sub-off", menu.selected.is_none())
            .on_click(move |_, _, cx| act(SubtitleAction::Off, cx))
            .child(mark(menu.selected.is_none()))
            .child(label("Off"))
    };
    let embedded = menu.embedded.iter().map(|track| {
        let (id, shown) = (track.id, menu.selected == Some(track.id));
        let act = act.clone();
        row(SharedString::from(format!("sub-track-{id}")), shown)
            .on_click(move |_, _, cx| act(SubtitleAction::Embedded(id), cx))
            .child(mark(shown))
            .child(label(&captions::track_name(track)))
            .children((!track.title.is_empty()).then(|| dim(&track.title)))
    });
    let online = menu.online.iter().map(|group| {
        let open = menu.expanded == Some(group.code.as_str());
        let in_use = group
            .options
            .iter()
            .any(|option| menu.online_in_use == Some(option.id.as_str()));
        let code = group.code.clone();
        let toggle = {
            let act = act.clone();
            row(
                SharedString::from(format!("sub-lang-{}", group.code)),
                false,
            )
            .on_click(move |_, _, cx| act(SubtitleAction::Language(code.clone()), cx))
            .child(mark(in_use))
            .child(label(&group.language))
            .child(div().flex_1())
            .child(dim(&group.options.len().to_string()))
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .small()
                .text_color(color(TEXT_DIM)),
            )
        };
        let options = open.then(|| {
            div()
                .flex()
                .flex_col()
                .pl_6()
                .children(group.options.iter().map(|option| {
                    let shown = menu.online_in_use == Some(option.id.as_str());
                    let (id, act) = (option.id.clone(), act.clone());
                    let name = if option.label.is_empty() {
                        option.provider.clone()
                    } else {
                        option.label.clone()
                    };
                    row(SharedString::from(format!("sub-{}", option.id)), shown)
                        .on_click(move |_, _, cx| act(SubtitleAction::Online(id.clone()), cx))
                        .child(mark(shown))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(color(TEXT))
                                        .truncate()
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(color(TEXT_FAINT))
                                        .child(option.provider.clone()),
                                ),
                        )
                        .children(option.hearing_impaired.then(|| badge("SDH", false)))
                }))
        });
        div().flex().flex_col().child(toggle).children(options)
    });

    let has_embedded = !menu.embedded.is_empty();
    let has_online = !menu.online.is_empty();
    div().flex_1().min_h_0().flex().flex_col().child(
        scroll_area("subtitles-scroll").child(
            div()
                .flex()
                .flex_col()
                .pb_3()
                .child(div().px_2().pt_1().child(off))
                .when(has_embedded, |list| {
                    list.child(section_label("In the video"))
                        .child(div().px_2().flex().flex_col().children(embedded))
                })
                .when(has_online, |list| {
                    list.child(section_label("Online"))
                        .child(div().px_2().flex().flex_col().children(online))
                }),
        ),
    )
}

fn label(text: &str) -> Div {
    div()
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color(TEXT))
        .child(text.to_string())
}

fn dim(text: &str) -> Div {
    div()
        .text_size(px(12.))
        .text_color(color(TEXT_DIM))
        .child(text.to_string())
}
