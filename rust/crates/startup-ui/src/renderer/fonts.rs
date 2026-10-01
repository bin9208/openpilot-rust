use super::*;
impl Renderer {
    pub fn set_language(&mut self, language: &str) {
        self.fallback = matches!(language, "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja");
    }

    pub(super) fn font_id(&self, font: Font) -> u32 {
        match font {
            Font::NormalRaw => self.normal,
            Font::Normal => {
                if self.fallback {
                    self.display.unwrap_or(self.normal)
                } else {
                    self.normal
                }
            }
            Font::Medium => {
                if self.fallback {
                    self.display.unwrap_or(self.medium)
                } else {
                    self.medium
                }
            }
            Font::Pretendard => {
                if self.fallback {
                    self.display.unwrap_or(self.pretendard)
                } else {
                    self.pretendard
                }
            }
            Font::Display => self.display.unwrap_or(self.normal),
            Font::Bold | Font::SemiBold | Font::Unifont | Font::Regular => {
                let name = match font {
                    Font::Bold => "Inter-Bold",
                    Font::SemiBold => "Inter-SemiBold",
                    Font::Unifont => "unifont",
                    _ => "Inter-Regular",
                };
                let id = self.fonts.get(name).copied().unwrap_or(self.normal);
                if self.fallback {
                    self.display.unwrap_or(id)
                } else {
                    id
                }
            }
        }
    }
}
pub(super) fn resolve_font(root: &Path, name: &str) -> PathBuf {
    let path = root.join(format!("{name}.fnt"));
    if path.exists() {
        return path;
    }
    for extension in ["ttf", "otf"] {
        let source = root.join(format!("{name}.{extension}"));
        if source.exists() {
            eprintln!(
                "Font atlas missing, loading source font instead: {}",
                source.display()
            );
            return source;
        }
    }
    path
}
pub(super) fn font_points(name: &str) -> Vec<i32> {
    let mut points: Vec<_> = (32..127).collect();
    if matches!(name, "unifont" | "KaiGenGothicKR-Bold") {
        points.extend(0xAC00..0xD7A4);
        points.extend(0x4E00..0xA000);
        points.extend(0x3400..0x4DC0);
        points.sort_unstable();
    }
    points
}
