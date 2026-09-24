//! `verify`: for each catalog model, do what a user would do, through the app's own engine:
//! download (sha256-checked), benchmark, and chat with thinking off and on. Writes a Markdown
//! report to catalog/VERIFICATION.md. Downloads are kept in `<dir>` and reused on later runs.

use opnlocal_engine::catalog::{ThinkingControl, UseCase};
use opnlocal_engine::engine::Event;
use opnlocal_engine::Engine;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

pub async fn run(dir: &Path, ids: &[String]) -> Result<(), String> {
    let last_pct = Arc::new(std::sync::Mutex::new(-1i64));
    let lp = last_pct.clone();
    let events: opnlocal_engine::engine::EventSink = Arc::new(move |e| {
        if let Event::DownloadProgress { model_id, progress: opnlocal_engine::download::Progress::Downloading { done, total, .. } } = e {
            let pct = (done * 100 / total.max(1)) as i64;
            let mut last = lp.lock().unwrap();
            if pct / 10 != *last / 10 {
                eprintln!("  {model_id}: {pct}%");
                *last = pct;
            }
        }
    });
    let engine = Arc::new(Engine::new(dir, None, env!("CARGO_PKG_VERSION"), events).map_err(|e| e.to_string())?);
    let device = engine.detect();
    let catalog = engine.state().models;
    let wanted: Vec<_> = catalog.iter().filter(|m| ids.is_empty() || ids.contains(&m.id)).collect();

    let mut report = String::new();
    writeln!(report, "# Catalog verification\n").unwrap();
    writeln!(report, "Catalog version {}. Run on {}, {} ({} GiB RAM){}.\n",
        engine.state().catalog_version,
        device.os_version,
        device.cpu.name,
        device.memory.total >> 30,
        device.gpus.iter().map(|g| format!(", {} ({} GiB)", g.name, g.memory_total >> 30)).collect::<String>()
    ).unwrap();
    writeln!(report, "Every model was downloaded from its pinned Hugging Face commit, checked against its sha256, loaded by the app's engine, benchmarked, and asked two questions (thinking off, then \"Think harder\").\n").unwrap();
    writeln!(report, "| Model | Placement | Words/s | Tokens/s | First word | Estimated need | Measured (RAM + GPU) | Plain answer | Think-harder answer |").unwrap();
    writeln!(report, "|---|---|---|---|---|---|---|---|---|").unwrap();

    let mut failures = Vec::new();
    for m in wanted {
        eprintln!("== {}", m.id);
        *last_pct.lock().unwrap() = -1;
        let row = verify_one(&engine, m).await;
        match row {
            Ok(r) => writeln!(report, "{r}").unwrap(),
            Err(e) => {
                eprintln!("  FAILED: {e}");
                failures.push(format!("{}: {e}", m.id));
                writeln!(report, "| {} | – | – | – | – | – | – | FAILED: {} | |", m.name, e.replace('|', "/")).unwrap();
            }
        }
        // Free the memory before the next model.
        engine.unload();
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog/VERIFICATION.md");
    std::fs::write(&path, &report).map_err(|e| e.to_string())?;
    println!("{report}");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} model(s) failed: {}", failures.len(), failures.join("; ")))
    }
}

async fn verify_one(engine: &Arc<Engine>, m: &opnlocal_engine::catalog::CatalogModel) -> Result<String, String> {
    if !engine.store().is_installed(&m.id) {
        if m.license.requires_acceptance {
            engine.accept_license(&m.license.id).map_err(|e| e.to_string())?;
        }
        engine.download(&m.id).await.map_err(|e| e.to_string())?;
    }
    let need = engine
        .recommend(UseCase::Everyday)
        .all
        .into_iter()
        .find(|o| o.model_id == m.id)
        .map(|o| o.fit.need_bytes)
        .unwrap_or(0);

    let e = engine.clone();
    let id = m.id.clone();
    let bench = tokio::task::spawn_blocking(move || e.benchmark(&id)).await.unwrap().map_err(|e| e.to_string())?;
    eprintln!("  bench: {:?} words/s on {}", bench.words_per_second, bench.device);

    engine.update_settings(|s| {
        s.active_model = Some(m.id.clone());
        s.use_case = Some(UseCase::Everyday);
    }).map_err(|e| e.to_string())?;

    let ask = |q: &'static str, harder: bool| {
        let e = engine.clone();
        async move {
            tokio::task::spawn_blocking(move || e.send_message(None, q, vec![], harder)).await.unwrap()
        }
    };
    let plain = ask("What is the capital of France? Answer with one word.", false).await.map_err(|e| e.to_string())?;
    let plain_msg = plain.messages.last().unwrap();
    let plain_ok = plain_msg.content.contains("Paris");
    // Thinking must stay out of normal replies.
    let plain_clean = plain_msg.thinking.is_none() || matches!(m.chat.thinking, ThinkingControl::AlwaysOn { .. });
    eprintln!("  plain: {:?} (thinking: {})", plain_msg.content, plain_msg.thinking.as_deref().map(|t| t.len()).unwrap_or(0));

    let hard = ask("What is 17 multiplied by 23? Give just the number at the end.", true).await.map_err(|e| e.to_string())?;
    let hard_msg = hard.messages.last().unwrap();
    let hard_ok = hard_msg.content.contains("391");
    let thought = hard_msg.thinking.as_ref().is_some_and(|t| !t.trim().is_empty());
    eprintln!("  hard: {:?} (thinking chars: {})", hard_msg.content.chars().take(80).collect::<String>(), hard_msg.thinking.as_deref().map(str::len).unwrap_or(0));

    let measured = bench.ram_in_use_bytes.unwrap_or(0) + bench.gpu_memory_used_bytes.unwrap_or(0);
    let gib = |b: u64| format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64);
    let expect_thinking = !matches!(m.chat.thinking, ThinkingControl::Unsupported);
    let mark = |ok: bool| if ok { "✓" } else { "✗" };
    Ok(format!(
        "| {} | {:?} ({} layers) | {} | {} | {:.1} s | {} | {} | {} {} | {} {}{} |",
        m.name,
        bench.placement,
        bench.gpu_layers,
        bench.words_per_second.map(|w| format!("{w:.1}")).unwrap_or("–".into()),
        bench.generation_tokens_per_second.map(|w| format!("{w:.1}")).unwrap_or("–".into()),
        bench.first_token_ms / 1000.0,
        gib(need),
        gib(measured),
        mark(plain_ok && plain_clean),
        short(&plain_msg.content),
        mark(hard_ok),
        short(&hard_msg.content),
        if expect_thinking { if thought { " (thought first)" } else { " (no thinking shown)" } } else { "" },
    ))
}

fn short(s: &str) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
    if one_line.chars().count() > 60 {
        format!("…{}", one_line.chars().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect::<String>())
    } else {
        one_line
    }
}
