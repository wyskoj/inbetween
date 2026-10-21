//! The interface fonts: Geist Regular and Medium (SIL OFL 1.1, the licence allowed for font
//! assets; see `ATTRIBUTION.md`), with Roboto Regular (Apache-2.0, also the caption font) behind
//! them for any character Geist lacks. egui's own default fonts carry licences outside our list,
//! so the `default_fonts` feature stays off. Medium is the family named [`crate::theme::MEDIUM`].

use std::sync::Arc;

const GEIST: &[u8] = include_bytes!("../assets/Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../assets/Geist-Medium.ttf");
const ROBOTO: &[u8] = inbetween_render_cpu::caption::DEFAULT_FONT;

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, data) in [
        ("geist", GEIST),
        ("geist-medium", GEIST_MEDIUM),
        ("roboto", ROBOTO),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(egui::FontData::from_static(data)));
    }
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.insert(0, "roboto".to_owned());
        list.insert(0, "geist".to_owned());
    }
    fonts.families.insert(
        egui::FontFamily::Name(crate::theme::MEDIUM.into()),
        vec!["geist-medium".to_owned(), "roboto".to_owned()],
    );
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundled_fonts_are_real_truetype_files() {
        // TrueType files start with the sfnt version 0x00010000.
        for font in [super::GEIST, super::GEIST_MEDIUM, super::ROBOTO] {
            assert_eq!(&font[..4], &[0, 1, 0, 0]);
            assert!(font.len() > 100_000);
        }
    }
}
