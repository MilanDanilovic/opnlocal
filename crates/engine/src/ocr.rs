//! Text from images (screenshots, photos of documents), read on the device with the ocrs
//! models built into the binary (see ocr/README.md). Printed text works well; handwriting and
//! stylised fonts mostly don't. Used for models that can't see images themselves.

use crate::docs::DocError;
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

static DETECTION: &[u8] = include_bytes!("../ocr/text-detection.rten");
static RECOGNITION: &[u8] = include_bytes!("../ocr/text-recognition.rten");

/// Longer sides are shrunk to this before reading: phone photos are far bigger than the
/// detector needs, and the time grows with the pixel count.
const MAX_SIDE: u32 = 2048;

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];

/// Re-encodes an image as JPEG with its longer side at most `max_side` pixels: what a model that
/// can see gets, and what is kept on disk.
pub fn shrink_image(image: &[u8], max_side: u32) -> Result<Vec<u8>, DocError> {
    let unreadable = |e: &dyn std::fmt::Display| DocError::Unreadable(e.to_string());
    let mut img = image::load_from_memory(image).map_err(|e| unreadable(&e))?;
    if img.width().max(img.height()) > max_side {
        img = img.resize(max_side, max_side, image::imageops::FilterType::Triangle);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.into_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| unreadable(&e))?;
    Ok(out.into_inner())
}

pub struct Ocr {
    engine: OcrEngine,
}

impl Ocr {
    pub fn load() -> Result<Ocr, String> {
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(Model::load_static_slice(DETECTION).map_err(|e| e.to_string())?),
            recognition_model: Some(Model::load_static_slice(RECOGNITION).map_err(|e| e.to_string())?),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        Ok(Ocr { engine })
    }

    /// The text in an encoded image (PNG, JPEG, WebP, …), one line per detected line, in
    /// reading order.
    pub fn read(&self, image: &[u8]) -> Result<String, DocError> {
        let unreadable = |e: &dyn std::fmt::Display| DocError::Unreadable(e.to_string());
        let mut img = image::load_from_memory(image).map_err(|e| unreadable(&e))?;
        if img.width().max(img.height()) > MAX_SIDE {
            img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
        }
        let rgb = img.into_rgb8();
        let source = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| unreadable(&e))?;
        let input = self.engine.prepare_input(source).map_err(|e| unreadable(&e))?;
        let words = self.engine.detect_words(&input).map_err(|e| unreadable(&e))?;
        let lines = self.engine.find_text_lines(&input, &words);
        let text = self.engine.recognize_text(&input, &lines).map_err(|e| unreadable(&e))?;
        let out: Vec<String> = text
            .iter()
            .flatten()
            .map(|l| l.to_string())
            // One-character "lines" are almost always noise.
            .filter(|l| l.trim().chars().count() > 1)
            .collect();
        Ok(out.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_printed_text_from_a_screenshot() {
        let ocr = Ocr::load().unwrap();
        let text = ocr.read(include_bytes!("../ocr/sample.png")).unwrap();
        assert!(text.contains("Hello from opnlocal"), "{text:?}");
        assert!(text.contains("14 October"), "{text:?}");
    }

    #[test]
    fn rejects_files_that_are_not_images() {
        let ocr = Ocr::load().unwrap();
        assert!(matches!(ocr.read(b"not an image"), Err(DocError::Unreadable(_))));
    }
}
