use anyhow::Result;
use qrcode::render::svg;
use qrcode::QrCode;

pub fn generate_qr_svg(data: &str) -> Result<String> {
    let code = QrCode::new(data.as_bytes())?;
    let svg = code.render::<svg::Color>().build();
    Ok(svg)
}

pub fn generate_qr_png(data: &str, _size: u32) -> Result<Vec<u8>> {
    let svg_str = generate_qr_svg(data)?;
    Ok(svg_str.into_bytes())
}
