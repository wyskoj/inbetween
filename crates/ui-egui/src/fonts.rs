//! The interface font. Roboto is Apache-2.0 (see `ATTRIBUTION.md`); egui's own default fonts
//! carry licences outside our permissive list, so the `default_fonts` feature stays off.

use std::sync::Arc;

const ROBOTO: &[u8] = inbetween_render_cpu::caption::DEFAULT_FONT;

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "roboto".to_owned(),
        Arc::new(egui::FontData::from_static(ROBOTO)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "roboto".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundled_font_is_a_real_truetype_file() {
        // TrueType files start with the sfnt version 0x00010000.
        assert_eq!(&super::ROBOTO[..4], &[0, 1, 0, 0]);
        assert!(super::ROBOTO.len() > 100_000);
    }
}
