use ab_glyph::FontRef;

const REGULAR_BYTES: &[u8] = include_bytes!("../../assets/LiberationSans-Regular.ttf");
const BOLD_BYTES: &[u8] = include_bytes!("../../assets/LiberationSans-Bold.ttf");

pub fn font_regular() -> FontRef<'static> {
    FontRef::try_from_slice(REGULAR_BYTES).expect("Error al cargar fuente regular embebida")
}

pub fn font_bold() -> FontRef<'static> {
    FontRef::try_from_slice(BOLD_BYTES).expect("Error al cargar fuente bold embebida")
}
