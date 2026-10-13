use super::{
    format_time, smooth_scale, Canvas, Error, Fonts, Layout, Rect, Result, ScaleBackend,
    ToolsState, OPTION_LABELS,
};
use num_traits::ToPrimitive;
use openpilot_sdl2::gui::{Font, Image};
const TEXT: [u8; 3] = [232, 235, 239];
const MUTED: [u8; 3] = [151, 158, 168];
const PANEL: [u8; 3] = [28, 32, 37];
const BORDER: [u8; 3] = [62, 68, 76];
const BLUE: [u8; 3] = [91, 165, 255];
fn limit(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}
fn draw_text(
    canvas: &mut Canvas,
    font: &Font,
    value: &str,
    color: [u8; 3],
    x: i32,
    y: i32,
) -> Result<()> {
    canvas.blit(&font.render(value, color)?, x, y);
    Ok(())
}
fn centered(
    canvas: &mut Canvas,
    font: &Font,
    value: &str,
    color: [u8; 3],
    rect: Rect,
) -> Result<()> {
    let image = font.render(value, color)?;
    canvas.blit(
        &image,
        rect.x + rect.width / 2 - image.width / 2,
        rect.y + rect.height / 2 - image.height / 2,
    );
    Ok(())
}
/// # Errors
/// Returns invalid replay state or an external font rendering error.
pub fn draw(
    canvas: &mut Canvas,
    state: &ToolsState,
    fonts: &Fonts,
    backend: ScaleBackend,
) -> Result<()> {
    state.check_times()?;
    let layout = Layout::new(canvas.width, canvas.height)?;
    canvas.fill([18, 21, 25]);
    draw_text(canvas, &fonts.title, "REPLAY TOOLS", TEXT, 24, 17)?;
    draw_text(
        canvas,
        &fonts.small,
        &limit(&state.status_text, 110),
        MUTED,
        184,
        21,
    )?;
    let folder = if state.current_log_folder.is_empty() {
        "--"
    } else {
        &state.current_log_folder
    };
    draw_text(
        canvas,
        &fonts.small,
        &limit(&format!("LOG: {folder}"), 135),
        TEXT,
        24,
        46,
    )?;
    for (rect, label) in [
        (layout.open, "OPEN LOG"),
        (layout.previous, "< PREV"),
        (layout.pause, if state.paused { "PLAY" } else { "PAUSE" }),
        (layout.next, "NEXT >"),
    ] {
        canvas.rect(rect, [38, 43, 49], false);
        canvas.rect(rect, BORDER, true);
        centered(canvas, &fonts.small, label, TEXT, rect)?;
    }
    content(canvas, state, fonts, &layout, backend)?;
    options(canvas, state, fonts, &layout)?;
    seek_bar(canvas, state, fonts, &layout)?;
    Ok(())
}
fn content(
    canvas: &mut Canvas,
    state: &ToolsState,
    fonts: &Fonts,
    layout: &Layout,
    backend: ScaleBackend,
) -> Result<()> {
    for rect in [layout.video, layout.debug] {
        canvas.rect(rect, PANEL, false);
        canvas.rect(rect, BORDER, true);
    }
    if let Some((bytes, width, height)) = state.video() {
        let scale = (f64::from(layout.video.width)
            / width
                .to_f64()
                .ok_or(Error::Contract("video width invalid"))?)
        .min(
            f64::from(layout.video.height)
                / height
                    .to_f64()
                    .ok_or(Error::Contract("video height invalid"))?,
        );
        let target = (
            (width
                .to_f64()
                .ok_or(Error::Contract("video width invalid"))?
                * scale)
                .trunc()
                .max(1.)
                .to_usize()
                .ok_or(Error::Contract("scaled width invalid"))?,
            (height
                .to_f64()
                .ok_or(Error::Contract("video height invalid"))?
                * scale)
                .trunc()
                .max(1.)
                .to_usize()
                .ok_or(Error::Contract("scaled height invalid"))?,
        );
        let image = smooth_scale(bytes, width, height, target, backend)?;
        canvas.blit(
            &image,
            layout.video.x + layout.video.width / 2 - image.width / 2,
            layout.video.y + layout.video.height / 2 - image.height / 2,
        );
    } else {
        centered(
            canvas,
            &fonts.normal,
            &limit(
                state
                    .video_status
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .unwrap_or("ROUTE VIDEO WAITING"),
                70,
            ),
            MUTED,
            layout.video,
        )?;
    }
    let x = layout.debug.x + 16;
    let mut y = layout.debug.y + 14;
    draw_text(canvas, &fonts.small, "LOG RADARSTATE CUT-IN", MUTED, x, y)?;
    y += 25;
    let cutin = state
        .cutin_status
        .as_deref()
        .filter(|v| !v.is_empty())
        .unwrap_or("waiting");
    draw_text(
        canvas,
        &fonts.normal,
        &limit(cutin, 45),
        if cutin.contains(": YES") {
            [255, 179, 62]
        } else {
            TEXT
        },
        x,
        y,
    )?;
    y += 34;
    for line in state.data_lines.iter().take(12) {
        draw_text(canvas, &fonts.small, &limit(line, 54), TEXT, x, y)?;
        y += 24;
        if y > layout.debug.y + layout.debug.height - 20 {
            break;
        }
    }
    Ok(())
}
fn options(canvas: &mut Canvas, state: &ToolsState, fonts: &Fonts, layout: &Layout) -> Result<()> {
    for ((name, label), (_, rect)) in OPTION_LABELS.iter().zip(&layout.options) {
        let checked = state.options.get(*name).copied().unwrap_or(false);
        let box_rect = Rect::new(rect.x, rect.y + 2, 16, 16);
        canvas.rect(box_rect, if checked { BLUE } else { PANEL }, false);
        canvas.rect(box_rect, BORDER, true);
        if checked {
            canvas.line2(
                (box_rect.x + 3, box_rect.y + 8),
                (box_rect.x + 7, box_rect.y + 12),
                [255; 3],
            );
            canvas.line2(
                (box_rect.x + 7, box_rect.y + 12),
                (box_rect.x + 14, box_rect.y + 4),
                [255; 3],
            );
        }
        draw_text(canvas, &fonts.small, label, TEXT, rect.x + 23, rect.y)?;
    }
    Ok(())
}
fn seek_bar(canvas: &mut Canvas, state: &ToolsState, fonts: &Fonts, layout: &Layout) -> Result<()> {
    let duration = state.duration_s.max(0.001);
    let playback = state.playback_s.min(duration).max(0.);
    let ratio = playback / duration;
    let progress = (f64::from(layout.seek.width) * ratio)
        .trunc()
        .to_i32()
        .ok_or(Error::Contract("seek progress invalid"))?;
    canvas.rect(layout.seek, [64, 70, 78], false);
    if ratio > 0. {
        canvas.rect(
            Rect::new(
                layout.seek.x,
                layout.seek.y,
                progress.max(2),
                layout.seek.height,
            ),
            BLUE,
            false,
        );
    }
    canvas.circle9(
        (
            layout.seek.x + progress,
            layout.seek.y + layout.seek.height / 2,
        ),
        TEXT,
    );
    draw_text(
        canvas,
        &fonts.normal,
        &format!("{} / {}", format_time(playback)?, format_time(duration)?),
        TEXT,
        layout.seek.x,
        layout.seek.y - 27,
    )?;
    Ok(())
}
/// # Errors
/// Returns invalid canvas dimensions or font rendering errors.
pub fn render(
    state: &ToolsState,
    fonts: &Fonts,
    width: i32,
    height: i32,
    backend: ScaleBackend,
) -> Result<Image> {
    let mut canvas = Canvas::new(width, height)?;
    draw(&mut canvas, state, fonts, backend)?;
    Ok(Image {
        width,
        height,
        rgba: canvas.rgba,
    })
}
