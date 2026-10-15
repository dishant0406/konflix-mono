//! What a file is, and the lightest way to make a browser play it.
//!
//! The rule is to change as little as possible, because every change costs CPU on a small
//! server and each step down the list below is the cheaper one to take:
//!
//!   1. An MP4 with H.264 video and AAC or MP3 audio is sent as it is. No work at all.
//!   2. Anything else with video a browser can decode is repackaged as HLS segments with the
//!      video copied untouched. The work is I/O only, and the picture is bit for bit the source.
//!   3. The audio is copied too when it is AAC, and turned into stereo AAC otherwise (FLAC, DTS and
//!      AC-3 do not play everywhere). That is a few percent of one core.
//!   4. Only video a browser cannot decode (10-bit H.264, HEVC on a browser that lacks it, AV1,
//!      MPEG-4 Part 2) is re-encoded, at a quality that is hard to tell from the source, and only
//!      one at a time.

use crate::origin::Origin;
use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::Value;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// What ffprobe is allowed to analyse once it has the header: enough frames to know the pixel
/// format and the audio. The header and a fansub's font attachments come before that and are not
/// counted, which is what `FEED_LIMIT` leaves room for.
const PROBE_BYTES: u64 = 8 * 1024 * 1024;
/// What this server will feed it. More than it analyses, because it reads the header and the
/// attachments before it starts counting; and bounded, because an endless read would download an
/// episode just to identify it. ffprobe stops reading when it has the streams, which ends the feed.
const FEED_LIMIT: u64 = 40 * 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(40);

#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub codec: String,
    pub profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub codec: String,
    pub channels: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    /// ffprobe's `format_name`, e.g. `matroska,webm` or `mov,mp4,m4a,3gp,3g2,mj2`.
    pub container: String,
    pub duration: Option<f64>,
    pub video: Option<VideoInfo>,
    pub audio: Option<AudioInfo>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    streams: Vec<Value>,
    format: Option<Value>,
}

/// Reads ffprobe's JSON. A number may arrive as a string, which ffprobe does for durations.
pub fn parse_probe(json: &str) -> anyhow::Result<Probe> {
    let raw: Raw = serde_json::from_str(json).context("ffprobe did not return JSON")?;
    let text =
        |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let number = |value: &Value, key: &str| {
        let found = value.get(key)?;
        found
            .as_u64()
            .or_else(|| found.as_str().and_then(|s| s.parse().ok()))
    };
    let kind = |value: &Value| text(value, "codec_type");
    let video = raw
        .streams
        .iter()
        .find(|stream| kind(stream).as_deref() == Some("video"))
        // A cover picture is a video stream too.
        .filter(|stream| {
            stream
                .pointer("/disposition/attached_pic")
                .and_then(Value::as_u64)
                != Some(1)
        })
        .and_then(|stream| {
            Some(VideoInfo {
                codec: text(stream, "codec_name")?,
                profile: text(stream, "profile"),
                pix_fmt: text(stream, "pix_fmt"),
                width: number(stream, "width").map(|w| w as u32),
                height: number(stream, "height").map(|h| h as u32),
            })
        });
    let audio = raw
        .streams
        .iter()
        .find(|stream| kind(stream).as_deref() == Some("audio"))
        .and_then(|stream| {
            Some(AudioInfo {
                codec: text(stream, "codec_name")?,
                channels: number(stream, "channels").map(|c| c as u32),
            })
        });
    let format = raw.format.unwrap_or(Value::Null);
    Ok(Probe {
        container: text(&format, "format_name").unwrap_or_default(),
        duration: format
            .get("duration")
            .and_then(|d| {
                d.as_f64()
                    .or_else(|| d.as_str().and_then(|s| s.parse().ok()))
            })
            .filter(|d| d.is_finite() && *d > 0.),
        video,
        audio,
    })
}

/// Identifies a file by reading its header through `origin`, never from a path: a torrent's file
/// on disk is sparse until its pieces arrive, so a tool reading the path stops at the first gap.
pub async fn probe(origin: &Origin) -> anyhow::Result<Probe> {
    let mut child = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-probesize",
            &PROBE_BYTES.to_string(),
            "-analyzeduration",
            "5000000",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
            "-i",
            "pipe:0",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("could not start ffprobe")?;
    let mut stdin = child.stdin.take().context("ffprobe has no stdin")?;
    let reader = origin.open_at(0).await?;

    // The child may finish before the whole head is written: it stops as soon as it has the
    // streams. Writing into a closed pipe is the expected end, not an error.
    let feed = async move {
        let mut head = tokio::io::AsyncReadExt::take(reader, FEED_LIMIT);
        match tokio::io::copy(&mut head, &mut stdin).await {
            Ok(copied) => tracing::debug!("[probe] fed {copied} bytes"),
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(error) => tracing::warn!("[probe] reading the file failed: {error}"),
        }
        let _ = stdin.shutdown().await;
    };
    let run = async {
        let output = child.wait_with_output().await?;
        anyhow::Ok(output)
    };
    let work = async {
        let (_, output) = tokio::join!(feed, run);
        output
    };
    let output = tokio::time::timeout(PROBE_TIMEOUT, work)
        .await
        .map_err(|_| anyhow::anyhow!("reading \"{}\" timed out", origin.name()))??;
    if !output.stderr.is_empty() {
        tracing::debug!(
            "[probe] ffprobe: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !output.status.success() && output.stdout.is_empty() {
        let reason = String::from_utf8_lossy(&output.stderr);
        bail!("could not read \"{}\": {}", origin.name(), reason.trim());
    }
    let probe = parse_probe(&String::from_utf8_lossy(&output.stdout))?;
    // ffprobe that could not make sense of the bytes still prints an empty answer.
    if probe.container.is_empty() {
        bail!("could not read \"{}\"", origin.name());
    }
    Ok(probe)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMode {
    /// Untouched.
    Copy,
    /// Untouched, tagged so Safari and Chrome treat it as HEVC.
    CopyHevc,
    /// Re-encoded to 8-bit H.264.
    Encode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMode {
    Copy,
    /// Stereo AAC.
    Aac,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    /// The file as it is, with range support.
    Direct,
    Hls {
        video: VideoMode,
        audio: AudioMode,
    },
    Unsupported(String),
}

/// 8-bit 4:2:0 in a profile every browser decodes.
fn plain_h264(video: &VideoInfo) -> bool {
    let pix_ok = matches!(
        video.pix_fmt.as_deref(),
        None | Some("yuv420p" | "yuvj420p")
    );
    let profile_ok = !matches!(
        video.profile.as_deref(),
        Some(profile) if profile.contains("High 10")
            || profile.contains("4:2:2")
            || profile.contains("4:4:4")
    );
    pix_ok && profile_ok
}

pub fn decide(probe: &Probe, client_plays_hevc: bool) -> Delivery {
    let Some(video) = &probe.video else {
        return Delivery::Unsupported("The file has no video.".into());
    };
    let audio_ok_in_mp4 = match &probe.audio {
        None => true,
        Some(audio) => matches!(audio.codec.as_str(), "aac" | "mp3"),
    };
    let is_mp4 = probe.container.split(',').any(|name| name == "mp4");

    if is_mp4 && video.codec == "h264" && plain_h264(video) && audio_ok_in_mp4 {
        return Delivery::Direct;
    }

    let video_mode = match video.codec.as_str() {
        "h264" if plain_h264(video) => VideoMode::Copy,
        "hevc"
            if client_plays_hevc
                && matches!(
                    video.pix_fmt.as_deref(),
                    None | Some("yuv420p" | "yuv420p10le")
                ) =>
        {
            VideoMode::CopyHevc
        }
        _ => VideoMode::Encode,
    };
    let audio_mode = match &probe.audio {
        None => AudioMode::None,
        Some(audio) if audio.codec == "aac" => AudioMode::Copy,
        Some(_) => AudioMode::Aac,
    };
    Delivery::Hls {
        video: video_mode,
        audio: audio_mode,
    }
}

/// The ffmpeg options that turn the chosen modes into output streams.
pub fn codec_args(video: VideoMode, audio: AudioMode, threads: usize) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| args.extend(items.iter().map(|item| item.to_string()));
    match video {
        VideoMode::Copy => push(&["-c:v", "copy"]),
        VideoMode::CopyHevc => push(&["-c:v", "copy", "-tag:v", "hvc1"]),
        VideoMode::Encode => {
            // Quality that is hard to tell from the source (CRF 22), at the fastest preset that
            // still holds it, in a pixel format every browser decodes. The picture is capped at
            // 720p: this path only exists for video that has to be re-encoded, the cost of an
            // encode follows the pixel count, and the step from 1080p to 720p is the one that is
            // hard to see on the screens this is watched on. A keyframe every six seconds lines
            // up with the segments.
            push(&[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "22",
                "-pix_fmt",
                "yuv420p",
                "-vf",
                "scale=-2:'min(720,ih)'",
                "-force_key_frames",
                "expr:gte(t,n_forced*6)",
                "-threads",
                &threads.to_string(),
                "-filter_threads",
                "1",
            ]);
        }
    }
    match audio {
        AudioMode::Copy => push(&["-c:a", "copy"]),
        AudioMode::Aac => push(&["-c:a", "aac", "-b:a", "160k", "-ac", "2"]),
        AudioMode::None => {}
    }
    args
}

/// Whether ffmpeg and ffprobe can be run, for `/health` and for the start-up check.
pub async fn ffmpeg_available() -> bool {
    for tool in ["ffmpeg", "ffprobe"] {
        let ok = Command::new(tool)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false);
        if !ok {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_of(container: &str, video: (&str, &str, &str), audio: Option<(&str, u32)>) -> Probe {
        Probe {
            container: container.into(),
            duration: Some(1400.),
            video: Some(VideoInfo {
                codec: video.0.into(),
                profile: Some(video.1.into()),
                pix_fmt: Some(video.2.into()),
                width: Some(1920),
                height: Some(1080),
            }),
            audio: audio.map(|(codec, channels)| AudioInfo {
                codec: codec.into(),
                channels: Some(channels),
            }),
        }
    }

    const MP4: &str = "mov,mp4,m4a,3gp,3g2,mj2";
    const MKV: &str = "matroska,webm";

    #[test]
    fn an_mp4_a_browser_plays_is_sent_untouched() {
        let probe = probe_of(MP4, ("h264", "High", "yuv420p"), Some(("aac", 2)));
        assert_eq!(decide(&probe, false), Delivery::Direct);
        let silent = probe_of(MP4, ("h264", "Main", "yuv420p"), None);
        assert_eq!(decide(&silent, false), Delivery::Direct);
    }

    #[test]
    fn an_mkv_with_h264_and_aac_copies_both() {
        let probe = probe_of(MKV, ("h264", "High", "yuv420p"), Some(("aac", 2)));
        assert_eq!(
            decide(&probe, false),
            Delivery::Hls {
                video: VideoMode::Copy,
                audio: AudioMode::Copy
            }
        );
    }

    #[test]
    fn only_the_audio_is_converted_when_the_video_is_fine() {
        for codec in ["flac", "dts", "ac3", "eac3", "opus", "vorbis", "truehd"] {
            let probe = probe_of(MKV, ("h264", "High", "yuv420p"), Some((codec, 6)));
            assert_eq!(
                decide(&probe, false),
                Delivery::Hls {
                    video: VideoMode::Copy,
                    audio: AudioMode::Aac
                },
                "{codec}"
            );
        }
        // An MP4 with AC-3 cannot be played as it is either.
        let probe = probe_of(MP4, ("h264", "High", "yuv420p"), Some(("ac3", 6)));
        assert_eq!(
            decide(&probe, false),
            Delivery::Hls {
                video: VideoMode::Copy,
                audio: AudioMode::Aac
            }
        );
    }

    #[test]
    fn ten_bit_h264_is_the_case_that_needs_an_encode() {
        let probe = probe_of(MKV, ("h264", "High 10", "yuv420p10le"), Some(("aac", 2)));
        assert_eq!(
            decide(&probe, true),
            Delivery::Hls {
                video: VideoMode::Encode,
                audio: AudioMode::Copy
            }
        );
        // And an MP4 of it is not played as it is.
        let probe = probe_of(MP4, ("h264", "High 10", "yuv420p10le"), Some(("aac", 2)));
        assert!(matches!(
            decide(&probe, true),
            Delivery::Hls {
                video: VideoMode::Encode,
                ..
            }
        ));
    }

    #[test]
    fn hevc_is_kept_only_for_a_browser_that_plays_it() {
        let probe = probe_of(MKV, ("hevc", "Main 10", "yuv420p10le"), Some(("aac", 2)));
        assert_eq!(
            decide(&probe, true),
            Delivery::Hls {
                video: VideoMode::CopyHevc,
                audio: AudioMode::Copy
            }
        );
        assert_eq!(
            decide(&probe, false),
            Delivery::Hls {
                video: VideoMode::Encode,
                audio: AudioMode::Copy
            }
        );
    }

    #[test]
    fn video_a_browser_cannot_decode_is_encoded() {
        for codec in ["av1", "mpeg4", "vp9", "mpeg2video", "vc1"] {
            let probe = probe_of(MKV, (codec, "", "yuv420p"), Some(("aac", 2)));
            assert!(
                matches!(
                    decide(&probe, true),
                    Delivery::Hls {
                        video: VideoMode::Encode,
                        ..
                    }
                ),
                "{codec}"
            );
        }
    }

    #[test]
    fn a_file_with_no_video_is_refused() {
        let mut probe = probe_of(MKV, ("h264", "High", "yuv420p"), None);
        probe.video = None;
        assert!(matches!(decide(&probe, false), Delivery::Unsupported(_)));
    }

    #[test]
    fn the_cheap_paths_never_start_an_encoder() {
        let copy = codec_args(VideoMode::Copy, AudioMode::Copy, 2).join(" ");
        assert_eq!(copy, "-c:v copy -c:a copy");
        let converted = codec_args(VideoMode::Copy, AudioMode::Aac, 2).join(" ");
        assert!(converted.contains("-c:v copy") && converted.contains("-c:a aac"));
        assert!(!converted.contains("libx264"));
        let hevc = codec_args(VideoMode::CopyHevc, AudioMode::None, 2).join(" ");
        assert!(hevc.contains("-tag:v hvc1") && !hevc.contains("-c:a"));
        let encode = codec_args(VideoMode::Encode, AudioMode::Aac, 3).join(" ");
        assert!(
            encode.contains("libx264")
                && encode.contains("-threads 3")
                && encode.contains("-crf 22")
        );
        assert!(encode.contains("min(720,ih)"));
    }

    #[test]
    fn ffprobe_json_is_read() {
        let json = r#"{
            "streams": [
              {"codec_type":"video","codec_name":"h264","profile":"High","pix_fmt":"yuv420p","width":1920,"height":1080},
              {"codec_type":"audio","codec_name":"aac","channels":2},
              {"codec_type":"subtitle","codec_name":"ass"}
            ],
            "format": {"format_name":"matroska,webm","duration":"1420.512000"}
        }"#;
        let probe = parse_probe(json).unwrap();
        assert_eq!(probe.container, "matroska,webm");
        assert_eq!(probe.duration, Some(1420.512));
        assert_eq!(probe.video.unwrap().codec, "h264");
        assert_eq!(probe.audio.unwrap().channels, Some(2));
    }

    #[test]
    fn a_cover_picture_is_not_the_video() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"mjpeg","disposition":{"attached_pic":1}}],"format":{"format_name":"mp3"}}"#;
        assert!(parse_probe(json).unwrap().video.is_none());
    }

    #[test]
    fn garbage_is_an_error_and_a_missing_duration_is_none() {
        assert!(parse_probe("not json").is_err());
        let probe =
            parse_probe(r#"{"streams":[],"format":{"format_name":"x","duration":"N/A"}}"#).unwrap();
        assert_eq!(probe.duration, None);
    }
}
