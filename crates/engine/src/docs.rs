//! Text from attached documents: .txt, .md, .pdf (text layer only, no OCR), .docx.
//! Pure Rust, so it works the same on every platform. Nothing leaves the device.

use std::io::Read;
use std::path::Path;

/// Larger files are refused before reading (they wouldn't fit any model's context anyway).
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum DocError {
    #[error("this file type isn't supported (use .txt, .md, .pdf, .docx or an image)")]
    Unsupported,
    #[error("the file is larger than 50 MB")]
    TooLarge,
    #[error("no text found in it")]
    NoText,
    #[error("the file couldn't be read: {0}")]
    Unreadable(String),
}

pub fn extract(path: &Path) -> Result<String, DocError> {
    let meta = std::fs::metadata(path).map_err(|e| DocError::Unreadable(e.to_string()))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(DocError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|e| DocError::Unreadable(e.to_string()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    extract_bytes(&ext, &bytes)
}

pub fn extract_bytes(ext: &str, bytes: &[u8]) -> Result<String, DocError> {
    let text = match ext {
        "txt" | "md" | "markdown" | "csv" | "log" => String::from_utf8_lossy(bytes).into_owned(),
        "pdf" => pdf_extract::extract_text_from_mem(bytes)
            .map_err(|e| DocError::Unreadable(e.to_string()))?,
        "docx" => docx_text(bytes)?,
        _ => return Err(DocError::Unsupported),
    };
    let text = normalize(&text);
    if text.trim().is_empty() {
        return Err(DocError::NoText);
    }
    Ok(text)
}

/// Paragraph text from word/document.xml: `<w:t>` runs, `<w:tab/>`, paragraph breaks.
fn docx_text(bytes: &[u8]) -> Result<String, DocError> {
    use quick_xml::events::Event;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| DocError::Unreadable(e.to_string()))?;
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|_| DocError::Unreadable("not a Word document".into()))?
        .read_to_string(&mut xml)
        .map_err(|e| DocError::Unreadable(e.to_string()))?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == "w:t" => in_text = true,
            Ok(Event::End(e)) if e.name().as_ref() == "w:t" => in_text = false,
            Ok(Event::End(e)) if e.name().as_ref() == "w:p" => out.push('\n'),
            Ok(Event::Empty(e)) if e.name().as_ref() == "w:tab" => out.push('\t'),
            Ok(Event::Empty(e)) if e.name().as_ref() == "w:br" => out.push('\n'),
            Ok(Event::Text(t)) if in_text => out.push_str(&t.into_inner()),
            Ok(Event::GeneralRef(r)) if in_text => {
                // Entities such as &amp; or &#8217; inside text runs.
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    out.push(c);
                    continue;
                }
                out.push_str(match r.into_inner().as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                });
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(DocError::Unreadable(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
}

/// Unifies line endings and collapses runs of 3+ blank lines (common in PDF output).
fn normalize(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn docx(body: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("word/document.xml", zip::write::SimpleFileOptions::default())
                .unwrap();
            write!(
                z,
                r#"<?xml version="1.0"?><w:document xmlns:w="w"><w:body>{body}</w:body></w:document>"#
            )
            .unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn plain_text_and_markdown() {
        assert_eq!(extract_bytes("txt", b"Hello\r\nworld").unwrap(), "Hello\nworld");
        assert_eq!(extract_bytes("MD".to_lowercase().as_str(), b"# Title\n\n\n\n\nBody").unwrap(), "# Title\n\nBody");
    }

    #[test]
    fn word_documents() {
        let d = docx(
            "<w:p><w:r><w:t>Quarterly report</w:t></w:r></w:p><w:p><w:r><w:t>Sales &amp; costs</w:t><w:tab/><w:t>up</w:t></w:r></w:p>",
        );
        assert_eq!(
            extract_bytes("docx", &d).unwrap(),
            "Quarterly report\nSales & costs\tup"
        );
    }

    #[test]
    fn empty_and_unsupported_files() {
        assert_eq!(extract_bytes("txt", b"  \n "), Err(DocError::NoText));
        assert_eq!(extract_bytes("xlsx", b"x"), Err(DocError::Unsupported));
        assert!(matches!(
            extract_bytes("docx", b"not a zip"),
            Err(DocError::Unreadable(_))
        ));
    }

    #[test]
    fn pdf_with_text_layer() {
        let pdf = minimal_pdf("Private notes about the budget");
        assert_eq!(
            extract_bytes("pdf", &pdf).unwrap(),
            "Private notes about the budget"
        );
    }

    /// A tiny valid PDF with one line of Helvetica text.
    fn minimal_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![];
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for off in offsets {
            out.extend(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}
