use crate::Result;
use std::{env, ffi::OsString, fs, os::unix::fs::MetadataExt, path::PathBuf};
use yazelix_venus::Color;

const DEFAULT_CURSOR_TAIL: (Color, f32) = (
    Color {
        r: 0x89,
        g: 0xb4,
        b: 0xfa,
    },
    1.0,
);
const USAGE: &str = "usage: yazelix-venus [--application-id ID] [--no-decorations] [--background-opacity VALUE] [--background-blur] [--cursor-effect-v1 none|tail] [--cursor-trail-color-v1 #RRGGBB --cursor-trail-duration-v1 0.25..4.0] [ORBIT_SOCKET [EON_WORKSPACE_SOCKET]]";

#[derive(Debug)]
pub(super) struct LaunchArguments {
    pub(super) application_id: String,
    pub(super) orbit_socket: PathBuf,
    pub(super) workspace_socket: Option<PathBuf>,
    pub(super) supervised: bool,
    pub(super) decorations: bool,
    pub(super) background_opacity: f32,
    pub(super) background_blur: bool,
    pub(super) cursor_tail: Option<(Color, f32)>,
}

pub(super) fn launch_arguments(
    arguments: impl IntoIterator<Item = OsString>,
    presentation_control: Option<OsString>,
) -> Result<LaunchArguments> {
    let mut arguments = arguments.into_iter();
    let mut application_id = None;
    let mut orbit_socket = None;
    let mut workspace_socket = None;
    let mut decorations = true;
    let mut background_opacity = None;
    let mut background_blur = false;
    let mut cursor_effect = None;
    let mut cursor_trail_color = None;
    let mut cursor_trail_duration = None;

    while let Some(argument) = arguments.next() {
        if argument == "--application-id" {
            if application_id.is_some() {
                return Err(USAGE.into());
            }
            application_id = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .filter(|value| valid_application_id(value));
            if application_id.is_none() {
                return Err(USAGE.into());
            }
        } else if argument == "--no-decorations" {
            decorations = false;
        } else if argument == "--background-opacity" {
            if background_opacity.is_some() {
                return Err(USAGE.into());
            }
            let Some(value) = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            else {
                return Err(USAGE.into());
            };
            background_opacity = Some(value);
        } else if argument == "--background-blur" {
            if background_blur {
                return Err(USAGE.into());
            }
            background_blur = true;
        } else if argument == "--cursor-effect-v1" {
            if cursor_effect.is_some() {
                return Err(USAGE.into());
            }
            cursor_effect = match arguments.next().as_deref() {
                Some(value) if value == "none" => Some(false),
                Some(value) if value == "tail" => Some(true),
                _ => return Err(USAGE.into()),
            };
        } else if argument == "--cursor-trail-color-v1" {
            if cursor_trail_color.is_some() {
                return Err(USAGE.into());
            }
            cursor_trail_color = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| parse_cursor_color(&value));
            if cursor_trail_color.is_none() {
                return Err(USAGE.into());
            }
        } else if argument == "--cursor-trail-duration-v1" {
            if cursor_trail_duration.is_some() {
                return Err(USAGE.into());
            }
            cursor_trail_duration = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.25..=4.0).contains(value));
            if cursor_trail_duration.is_none() {
                return Err(USAGE.into());
            }
        } else if argument.as_encoded_bytes().starts_with(b"-") {
            return Err(USAGE.into());
        } else if orbit_socket.is_none() {
            orbit_socket = Some(PathBuf::from(argument));
        } else if workspace_socket.is_none() {
            workspace_socket = Some(PathBuf::from(argument));
        } else {
            return Err(USAGE.into());
        }
    }

    let cursor_tail = match (cursor_effect, cursor_trail_color, cursor_trail_duration) {
        (None, None, None) => Some(DEFAULT_CURSOR_TAIL),
        (Some(false), None, None) => None,
        (Some(true), Some(color), Some(duration)) => Some((color, duration)),
        _ => return Err(USAGE.into()),
    };

    Ok(LaunchArguments {
        application_id: application_id.unwrap_or_else(|| "eon".into()),
        orbit_socket: orbit_socket.map_or_else(default_socket_path, Ok)?,
        workspace_socket,
        supervised: presentation_control == Some(OsString::from("stdin")),
        decorations,
        background_opacity: background_opacity.unwrap_or(1.0),
        background_blur,
        cursor_tail,
    })
}

fn valid_application_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn parse_cursor_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(Color {
        r: (rgb >> 16) as u8,
        g: (rgb >> 8) as u8,
        b: rgb as u8,
    })
}

fn default_socket_path() -> Result<PathBuf> {
    if let Some(root) = env::var_os("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(root).join("yazelix-orbit/orbit.sock"));
    }
    #[cfg(target_os = "linux")]
    let uid = fs::metadata("/proc/self")?.uid();
    #[cfg(not(target_os = "linux"))]
    let uid: u32 = env::var("UID")?.parse()?;
    Ok(PathBuf::from(format!(
        "/tmp/yazelix-orbit-{uid}/orbit.sock"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_complete_bounded_and_defaulted() {
        let parse =
            |arguments: &[&str]| launch_arguments(arguments.iter().map(OsString::from), None);

        let default = parse(&[]).unwrap();
        assert_eq!(default.application_id, "eon");
        assert!(default.decorations && default.workspace_socket.is_none());
        assert!(!default.supervised);
        assert_eq!(default.background_opacity, 1.0);
        assert!(!default.background_blur);
        assert_eq!(
            default.cursor_tail,
            Some((
                Color {
                    r: 0x89,
                    g: 0xb4,
                    b: 0xfa,
                },
                1.0,
            ))
        );

        let supervised =
            launch_arguments(Vec::<OsString>::new(), Some(OsString::from("stdin"))).unwrap();
        assert!(supervised.supervised);
        let other_control =
            launch_arguments(Vec::<OsString>::new(), Some(OsString::from("STDIN"))).unwrap();
        assert!(!other_control.supervised);

        for value in ["0", "0.88", "1"] {
            let parsed = parse(&[
                "--no-decorations",
                "--background-opacity",
                value,
                "--background-blur",
                "orbit.sock",
                "eon.sock",
            ])
            .unwrap();
            assert!(!parsed.decorations);
            assert_eq!(parsed.background_opacity, value.parse::<f32>().unwrap());
            assert!(parsed.background_blur);
            assert_eq!(parsed.orbit_socket, PathBuf::from("orbit.sock"));
            assert_eq!(parsed.workspace_socket, Some(PathBuf::from("eon.sock")));
        }

        let tail = parse(&[
            "--cursor-effect-v1",
            "tail",
            "--cursor-trail-color-v1",
            "#12aBcF",
            "--cursor-trail-duration-v1",
            "2.5",
            "orbit.sock",
        ])
        .unwrap();
        assert_eq!(
            tail.cursor_tail,
            Some((
                Color {
                    r: 0x12,
                    g: 0xab,
                    b: 0xcf,
                },
                2.5,
            ))
        );
        assert_eq!(
            parse(&["--cursor-effect-v1", "none"]).unwrap().cursor_tail,
            None
        );
        assert_eq!(
            parse(&["--application-id", "eonova"])
                .unwrap()
                .application_id,
            "eonova"
        );

        for invalid in [
            &["--unknown"][..],
            &["one", "two", "three"][..],
            &["--background-opacity"][..],
            &["--background-opacity", "bad"][..],
            &["--background-opacity", "NaN"][..],
            &["--background-opacity", "inf"][..],
            &["--background-opacity", "-0.01"][..],
            &["--background-opacity", "1.01"][..],
            &["--background-opacity", "0.5", "--background-opacity", "0.6"][..],
            &["--background-blur", "--background-blur"][..],
            &["--application-id"][..],
            &["--application-id", ""][..],
            &["--application-id", "bad/id"][..],
            &["--application-id", "eon", "--application-id", "eonova"][..],
            &["--cursor-effect-v1"][..],
            &["--cursor-effect-v1", "warp"][..],
            &["--cursor-effect-v1", "tail"][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
            ][..],
            &[
                "--cursor-effect-v1",
                "none",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &["--cursor-trail-color-v1", "#123456"][..],
            &["--cursor-trail-color-v1", "#aéabc"][..],
            &["--cursor-trail-color-v1", "123456"][..],
            &["--cursor-trail-color-v1", "#12345g"][..],
            &["--cursor-trail-duration-v1", "1"][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "0.24",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "4.01",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "NaN",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-color-v1",
                "#abcdef",
                "--cursor-trail-duration-v1",
                "1",
            ][..],
            &[
                "--cursor-effect-v1",
                "tail",
                "--cursor-trail-color-v1",
                "#123456",
                "--cursor-trail-duration-v1",
                "1",
                "--cursor-trail-duration-v1",
                "2",
            ][..],
        ] {
            assert_eq!(parse(invalid).unwrap_err().to_string(), USAGE);
        }
    }
}
