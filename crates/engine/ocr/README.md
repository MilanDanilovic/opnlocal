# OCR models

Text detection and recognition models from the [ocrs](https://github.com/robertknight/ocrs)
project (Robert Knight, Apache-2.0 / MIT), built into the engine binary by `src/ocr.rs` so
reading text from images works offline on every platform with nothing to download.

| File | Source | Size | sha256 |
|---|---|---|---|
| `text-detection.rten` | https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten | 2.5 MB | `f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca` |
| `text-recognition.rten` | https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten | 9.7 MB | `e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e` |

Downloaded 2026-09-25. The models were trained by the ocrs author on synthetic and public
datasets (see [ocrs-models](https://github.com/robertknight/ocrs-models)). They read printed
Latin-script text; handwriting, and scripts such as Cyrillic, are not covered.

`sample.png` is a generated test image used by the tests in `src/ocr.rs`.
