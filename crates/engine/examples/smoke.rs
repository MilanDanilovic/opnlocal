//! Build smoke test: list devices, load a GGUF, generate a few tokens.
//! Usage: cargo run -p opnlocal-engine --example smoke -- <model.gguf> [gpu_layers]

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use std::num::NonZeroU32;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let gpu_layers: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let mut backend = LlamaBackend::init().unwrap();
    backend.void_logs();
    llama_cpp_2::llama_backend::load_backends();
    for d in llama_cpp_2::list_llama_ggml_backend_devices() {
        println!(
            "device {}: {} / {} ({}) {:?} free={}MB total={}MB",
            d.index,
            d.name,
            d.description,
            d.backend,
            d.device_type,
            d.memory_free / 1_048_576,
            d.memory_total / 1_048_576
        );
    }

    let t = std::time::Instant::now();
    let model = LlamaModel::load_from_file(
        &backend,
        path,
        &LlamaModelParams::default().with_n_gpu_layers(gpu_layers),
    )
    .unwrap();
    println!("loaded in {:?}", t.elapsed());

    let mut ctx = model
        .new_context(
            &backend,
            LlamaContextParams::default().with_n_ctx(NonZeroU32::new(2048)),
        )
        .unwrap();
    let tokens = model
        .str_to_token("The capital of France is", AddBos::Always)
        .unwrap();
    let mut batch = LlamaBatch::new(512, 1);
    let last = tokens.len() as i32 - 1;
    for (i, tok) in tokens.iter().enumerate() {
        batch.add(*tok, i as i32, &[0], i as i32 == last).unwrap();
    }
    ctx.decode(&mut batch).unwrap();

    let mut sampler = LlamaSampler::greedy();
    let mut pos = tokens.len() as i32;
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut out = String::new();
    for _ in 0..32 {
        let tok = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(tok);
        if model.is_eog_token(tok) {
            break;
        }
        out.push_str(&model.token_to_piece(tok, &mut decoder, true, None).unwrap());
        batch.clear();
        batch.add(tok, pos, &[0], true).unwrap();
        pos += 1;
        ctx.decode(&mut batch).unwrap();
    }
    println!("output: {out}");
    let tm = ctx.timings();
    println!(
        "prompt {} tok in {:.0}ms, gen {} tok in {:.0}ms",
        tm.n_p_eval(),
        tm.t_p_eval_ms(),
        tm.n_eval(),
        tm.t_eval_ms()
    );
}
