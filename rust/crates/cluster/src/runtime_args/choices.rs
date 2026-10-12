use super::value::ArgumentValue;
use serde::Serialize;

macro_rules! choices {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
        pub enum $name { $(#[serde(rename = $text)] $variant),+ }
        impl ArgumentValue for $name {
            fn parse_argument(value: &str) -> Result<Self, String> {
                match value {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(format!("invalid choice: {value:?} (choose from {})", [$($text),+].join(", "))),
                }
            }
        }
    };
}
choices!(InputSource { Random => "random", Gamepad => "gamepad", Route => "route", Live => "live", Navi => "navi" });
choices!(Output { Usb => "usb", Window => "window", Both => "both" });
choices!(Codec { Jpeg => "jpeg", Png => "png", H264 => "h264" });
choices!(Theme { Dark => "dark", Auto => "auto", Light => "light" });
choices!(JpegEncoder { Auto => "auto", Pillow => "pillow", Turbojpeg => "turbojpeg" });
choices!(H264Backend { Auto => "auto", Native => "native", Ffmpeg => "ffmpeg" });
choices!(InputFormat { Auto => "auto", Nv12 => "nv12" });
choices!(RateControl { Off => "off", VbrVfr => "vbr-vfr", VbrCfr => "vbr-cfr", CbrVfr => "cbr-vfr", CbrCfr => "cbr-cfr", MbrCfr => "mbr-cfr", MbrVfr => "mbr-vfr", Cq => "cq" });
choices!(Orientation { Landscape => "landscape", Portrait => "portrait" });
choices!(RouteLog { Qlog => "qlog", Rlog => "rlog" });
choices!(CornerSource { Stable => "stable", Raw => "raw", Live => "live" });
choices!(RouteOverlay { Compact => "compact", Full => "full", Off => "off" });
choices!(RouteTools { Overlay => "overlay", Separate => "separate", Off => "off" });
choices!(Language { Ko => "ko", En => "en" });
