use super::*;
impl Renderer {
    pub fn set_language(&mut self, language: &str) {
        self.fallback = matches!(language, "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja");
    }

    pub fn font_for_text(&self, font: Font, text: &str) -> Font {
        if font != Font::NormalRaw && use_display(self.fallback, self.display.is_some(), text) {
            Font::Display
        } else {
            font
        }
    }
    pub(super) fn font_id_for_text(&self, font: Font, text: &str) -> u32 {
        self.font_id(self.font_for_text(font, text))
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
    points.extend(
        "–‑✓×°§•X⚙✕◀▶✔⌫⇧␣○●↳çêüñ–‑✓×°§•€£¥"
            .chars()
            .map(|c| c as i32),
    );
    if matches!(name, "unifont" | "KaiGenGothicKR-Bold") {
        points.extend(0xAC00..0xD7A4);
        points.extend(0x1100..0x1200);
        points.extend(0x3130..0x3190);
        points.extend(0x4E00..0xA000);
        points.extend(0x3400..0x4DC0);
    }
    points.sort_unstable();
    points.dedup();
    points
}

fn use_display(locale: bool, available: bool, text: &str) -> bool {
    available && (locale || text.chars().any(|c|matches!(c, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7a3}')))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_final_codepoints_and_locale_text_fallback() {
        let source: serde_json::Value =
            serde_json::from_str(include_str!("../../../../fixtures/ui-font-final.json")).unwrap();
        for row in source["fallback"].as_array().unwrap() {
            assert_eq!(
                use_display(
                    row[0].as_bool().unwrap(),
                    row[1].as_bool().unwrap(),
                    row[2].as_str().unwrap()
                ),
                row[3].as_bool().unwrap()
            );
        }
        for (name, points) in source["points"].as_object().unwrap() {
            assert_eq!(serde_json::to_value(font_points(name)).unwrap(), *points);
        }
    }
}
