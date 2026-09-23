//! `verify`: download each catalog model, load it with the app's engine and chat with it.

use std::path::Path;

pub async fn run(_dir: &Path, _ids: &[String]) -> Result<(), String> {
    Err("verify is not implemented yet".into())
}
