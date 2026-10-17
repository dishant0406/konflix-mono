//! The pictures of the music genres, and the assets of the app: the genre motifs are drawn as shapes
//! in a colour of the tile's choosing, so they ship as SVG files inside the binary, beside the library's
//! own icons.

use gpui_kit::assets::AllAssets;
use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

const MOTIFS: [(&str, &str); 12] = [
    ("pop", include_str!("../assets/genres/pop.svg")),
    ("hip-hop", include_str!("../assets/genres/hip-hop.svg")),
    ("rock", include_str!("../assets/genres/rock.svg")),
    ("bollywood", include_str!("../assets/genres/bollywood.svg")),
    (
        "electronic",
        include_str!("../assets/genres/electronic.svg"),
    ),
    ("rnb", include_str!("../assets/genres/rnb.svg")),
    ("indie", include_str!("../assets/genres/indie.svg")),
    ("jazz", include_str!("../assets/genres/jazz.svg")),
    ("kpop", include_str!("../assets/genres/kpop.svg")),
    ("lofi", include_str!("../assets/genres/lofi.svg")),
    ("classical", include_str!("../assets/genres/classical.svg")),
    ("metal", include_str!("../assets/genres/metal.svg")),
];

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path
            .strip_prefix("genres/")
            .and_then(|file| file.strip_suffix(".svg"))
            && let Some((_, svg)) = MOTIFS.iter().find(|(slug, _)| *slug == name)
        {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
        AllAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        AllAssets.list(path)
    }
}
