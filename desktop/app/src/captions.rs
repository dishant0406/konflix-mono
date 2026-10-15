//! Subtitles from outside the video: how they are grouped for the menu, which one to start
//! with, the language the viewer last chose, and the downloaded files mpv reads.

use crate::player::SubtitleTrack;
use akidev_core::subtitles::{self, Subtitle};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Downloaded subtitle files kept between runs. They are small and a viewer often reopens the
/// same episode, but the folder must not grow for ever.
const KEPT_FILES: usize = 40;

/// Every subtitle of one language, in the order the sources gave them.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub code: String,
    pub language: String,
    pub options: Vec<Subtitle>,
}

/// The sources' answer as one group per language. `find` already puts a language's subtitles
/// together with English first, so a group is a run; a language seen again later is merged
/// into its first group anyway.
pub fn group(found: Vec<Subtitle>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for subtitle in found {
        match groups.iter_mut().find(|group| group.code == subtitle.lang) {
            Some(group) => group.options.push(subtitle),
            None => groups.push(Group {
                code: subtitle.lang.clone(),
                language: subtitle.language.clone(),
                options: vec![subtitle],
            }),
        }
    }
    groups
}

/// "en", "pt-BR" and "eng" all start with the language they are for.
fn base(code: &str) -> String {
    let shown = subtitles::language(code).map_or(code, |(shown, _)| shown);
    shown
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Whether a track the file spells "eng" is in the language an online subtitle calls "en".
pub fn same_language(track_lang: &str, code: &str) -> bool {
    !track_lang.is_empty() && base(track_lang) == base(code)
}

/// "English" for "eng", or the code as the file has it.
pub fn track_name(track: &SubtitleTrack) -> String {
    match subtitles::language(&track.lang) {
        Some((_, name)) => name.to_string(),
        None if track.lang.is_empty() => "Unknown".to_string(),
        None => track.lang.to_uppercase(),
    }
}

/// What to show on its own when a video starts.
#[derive(Debug, Clone, PartialEq)]
pub enum Start<'a> {
    /// A track inside the file.
    Embedded(i64),
    /// An online subtitle, by its id.
    Online(&'a Subtitle),
}

/// The viewer's language, on a new video. A track in the file in that language wins, because it
/// is made for exactly this release and costs nothing to load. Otherwise the first online
/// subtitle of the language, preferring one that is not for the hard of hearing (the "SDH"
/// ones describe sounds, which is not what someone who chose a language wants by default).
/// Nothing when there is neither: mpv's own choice stands.
pub fn start_with<'a>(
    language: &str,
    embedded: &[&SubtitleTrack],
    online: &'a [Group],
) -> Option<Start<'a>> {
    if let Some(track) = embedded
        .iter()
        .find(|track| same_language(&track.lang, language))
    {
        return Some(Start::Embedded(track.id));
    }
    let group = online
        .iter()
        .find(|group| same_language(&group.code, language))?;
    group
        .options
        .iter()
        .find(|option| !option.hearing_impaired)
        .or(group.options.first())
        .map(Start::Online)
}

/// The language the viewer last chose, kept in a small file so the next video starts with it.
pub struct Remembered {
    path: PathBuf,
    language: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    language: Option<String>,
}

impl Remembered {
    /// A missing or unreadable file is no preference.
    pub fn load(path: PathBuf) -> Remembered {
        let language = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
            .and_then(|saved| saved.language);
        Remembered { path, language }
    }

    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// `None` is "Off": the viewer does not want subtitles started for them.
    pub fn set(&mut self, language: Option<String>) {
        if self.language == language {
            return;
        }
        self.language = language;
        let text = serde_json::to_string(&Saved {
            language: self.language.clone(),
        })
        .expect("a language serializes");
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(&self.path, text) {
            tracing::warn!("[captions] could not save {}: {error}", self.path.display());
        }
    }
}

/// Where the file for a subtitle address is kept: the same address is the same file.
pub fn cache_file(dir: &Path, url: &str) -> PathBuf {
    use std::hash::{DefaultHasher, Hash as _, Hasher as _};
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    dir.join(format!("{:016x}.vtt", hasher.finish()))
}

/// The subtitle as a WebVTT file mpv can load, downloaded unless it already is.
pub async fn download(dir: PathBuf, url: String) -> Result<PathBuf, String> {
    let path = cache_file(&dir, &url);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 0) {
        return Ok(path);
    }
    let text = subtitles::fetch(&url)
        .await
        .map_err(|error| format!("{error:#}"))?;
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Deletes all but the newest few files, at start, so the folder stays small.
pub fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in files.into_iter().skip(KEPT_FILES) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn online(id: &str, lang: &str, language: &str, sdh: bool) -> Subtitle {
        Subtitle {
            id: id.into(),
            provider: "p".into(),
            lang: lang.into(),
            language: language.into(),
            label: id.into(),
            url: format!("https://example.com/{id}"),
            format: None,
            hearing_impaired: sdh,
        }
    }

    fn track(id: i64, lang: &str) -> SubtitleTrack {
        SubtitleTrack {
            id,
            lang: lang.into(),
            title: String::new(),
            external: false,
        }
    }

    #[test]
    fn groups_by_language_in_order() {
        let groups = group(vec![
            online("a", "en", "English", false),
            online("b", "en", "English", true),
            online("c", "fr", "French", false),
        ]);
        let shape: Vec<_> = groups
            .iter()
            .map(|g| (g.code.as_str(), g.options.len()))
            .collect();
        assert_eq!(shape, [("en", 2), ("fr", 1)]);
        assert!(group(Vec::new()).is_empty());
    }

    #[test]
    fn matches_three_letter_and_regional_codes() {
        assert!(same_language("eng", "en"));
        assert!(same_language("por", "pt-BR"));
        assert!(same_language("pt", "pt-BR"));
        assert!(!same_language("eng", "fr"));
        assert!(!same_language("", "en"));
    }

    #[test]
    fn starts_with_the_file_own_track_in_that_language() {
        let french = track(2, "fre");
        let groups = group(vec![online("c", "fr", "French", false)]);
        assert_eq!(
            start_with("fr", &[&french], &groups),
            Some(Start::Embedded(2))
        );
    }

    #[test]
    fn starts_with_an_online_one_when_the_file_has_none_in_that_language() {
        let english = track(1, "eng");
        let groups = group(vec![
            online("a", "fr", "French", true),
            online("b", "fr", "French", false),
        ]);
        let picked = start_with("fr", &[&english], &groups);
        assert_eq!(
            picked.map(|start| match start {
                Start::Online(subtitle) => subtitle.id.clone(),
                Start::Embedded(_) => "embedded".into(),
            }),
            Some("b".into())
        );
    }

    #[test]
    fn falls_back_to_the_hearing_impaired_one_when_it_is_all_there_is() {
        let groups = group(vec![online("a", "fr", "French", true)]);
        assert!(matches!(
            start_with("fr", &[], &groups),
            Some(Start::Online(subtitle)) if subtitle.id == "a"
        ));
    }

    #[test]
    fn starts_with_nothing_when_no_one_has_the_language() {
        let groups = group(vec![online("a", "de", "German", false)]);
        assert_eq!(start_with("fr", &[&track(1, "eng")], &groups), None);
    }

    #[test]
    fn names_a_file_after_its_address() {
        let dir = Path::new("/x");
        assert_eq!(
            cache_file(dir, "https://a/1"),
            cache_file(dir, "https://a/1")
        );
        assert_ne!(
            cache_file(dir, "https://a/1"),
            cache_file(dir, "https://a/2")
        );
        assert!(cache_file(dir, "u").extension().is_some_and(|e| e == "vtt"));
    }

    #[test]
    fn remembers_a_language_between_runs() {
        let dir = std::env::temp_dir().join(format!("akidev-captions-{}", std::process::id()));
        let path = dir.join("subtitles.json");
        let mut remembered = Remembered::load(path.clone());
        assert_eq!(remembered.language(), None);
        remembered.set(Some("fr".into()));
        assert_eq!(Remembered::load(path.clone()).language(), Some("fr"));
        remembered.set(None);
        assert_eq!(Remembered::load(path).language(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
