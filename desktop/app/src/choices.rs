//! How the source picker words and orders the releases it lists. Plain functions, so the
//! panel itself has nothing left to decide.

use akidev_core::stream::Choice;

/// 1080 as "1080p", 2160 as "4K".
pub fn resolution_label(lines: u32) -> String {
    if lines >= 2160 {
        "4K".to_string()
    } else {
        format!("{lines}p")
    }
}

pub fn codec_label(codec: &str) -> Option<&'static str> {
    match codec {
        "h264" => Some("H.264"),
        "hevc" => Some("HEVC"),
        "av1" => Some("AV1"),
        _ => None,
    }
}

/// Lossless tracks are the ones worth telling apart; the rest is plain stereo to a viewer.
pub fn audio_label(audio: &str) -> Option<&'static str> {
    match audio {
        "lossless" => Some("Lossless"),
        "compressed" => Some("Stereo"),
        _ => None,
    }
}

/// 1_400_000_000 as "1.4 GB", 700_000_000 as "700 MB". Decimal units, as torrents and disks
/// label them.
pub fn size_label(bytes: u64) -> String {
    const MB: f64 = 1_000_000.;
    let megabytes = bytes as f64 / MB;
    if megabytes >= 1000. {
        format!("{:.1} GB", megabytes / 1000.)
    } else {
        format!("{megabytes:.0} MB")
    }
}

/// The quiet badges under a release's name, in reading order.
pub fn badges(choice: &Choice) -> Vec<String> {
    let mut badges = Vec::new();
    badges.extend(choice.resolution.map(resolution_label));
    badges.extend(choice.codec.and_then(codec_label).map(str::to_string));
    badges.extend(choice.audio.and_then(audio_label).map(str::to_string));
    badges.extend(choice.size_bytes.map(size_label));
    badges.push(format!("{} seeders", choice.seeders));
    if choice.pack {
        badges.push("Season pack".to_string());
    }
    badges
}

/// The releases in the order they were ranked, split into the usual ones and the ones in
/// other formats (HEVC, AV1) that go under their own divider.
pub fn split(choices: &[Choice]) -> (Vec<&Choice>, Vec<&Choice>) {
    choices.iter().partition(|choice| !choice.uncommon)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(title: &str) -> Choice {
        Choice {
            id: title.to_string(),
            title: title.to_string(),
            size_bytes: Some(1_400_000_000),
            seeders: 42,
            source: "x".into(),
            resolution: Some(1080),
            codec: Some("h264"),
            audio: Some("compressed"),
            pack: false,
            uncommon: false,
        }
    }

    #[test]
    fn names_resolutions() {
        assert_eq!(resolution_label(720), "720p");
        assert_eq!(resolution_label(1080), "1080p");
        assert_eq!(resolution_label(2160), "4K");
    }

    #[test]
    fn names_sizes() {
        assert_eq!(size_label(1_400_000_000), "1.4 GB");
        assert_eq!(size_label(700_000_000), "700 MB");
        assert_eq!(size_label(999_000_000), "999 MB");
        assert_eq!(size_label(12_340_000_000), "12.3 GB");
    }

    #[test]
    fn names_codecs_and_audio() {
        assert_eq!(codec_label("hevc"), Some("HEVC"));
        assert_eq!(codec_label("h264"), Some("H.264"));
        assert_eq!(codec_label("av1"), Some("AV1"));
        assert_eq!(codec_label("vp9"), None);
        assert_eq!(audio_label("lossless"), Some("Lossless"));
        assert_eq!(audio_label("compressed"), Some("Stereo"));
    }

    #[test]
    fn lists_badges_in_order() {
        let mut release = choice("a");
        release.pack = true;
        assert_eq!(
            badges(&release),
            [
                "1080p",
                "H.264",
                "Stereo",
                "1.4 GB",
                "42 seeders",
                "Season pack"
            ]
        );
    }

    #[test]
    fn leaves_out_what_is_unknown() {
        let mut release = choice("a");
        (
            release.resolution,
            release.codec,
            release.audio,
            release.size_bytes,
        ) = (None, None, None, None);
        assert_eq!(badges(&release), ["42 seeders"]);
    }

    #[test]
    fn puts_other_formats_after_the_rest_keeping_order() {
        let mut hevc = choice("hevc");
        hevc.uncommon = true;
        let all = [choice("one"), hevc, choice("two")];
        let (usual, other) = split(&all);
        let ids = |list: &[&Choice]| list.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&usual), ["one", "two"]);
        assert_eq!(ids(&other), ["hevc"]);
    }
}
