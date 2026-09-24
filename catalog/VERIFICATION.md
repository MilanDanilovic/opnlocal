# Catalog verification

Catalog version 1. Run on Windows 11 Pro, AMD Ryzen 7 7700X 8-Core Processor (31 GiB RAM), AMD Radeon RX 7800 XT (15 GiB), AMD Radeon(TM) Graphics (15 GiB).

Every model was downloaded from its pinned Hugging Face commit, checked against its sha256, loaded by the app's engine, benchmarked, and asked two questions (thinking off, then "Think harder").

| Model | Placement | Words/s | Tokens/s | First word | Estimated need | Measured (RAM + GPU) | Plain answer | Think-harder answer |
|---|---|---|---|---|---|---|---|---|
| Qwen3.5 0.8B | Cpu (-1 layers) | 269.1 | 311.4 | 0.3 s | 1.8 GiB | 1.0 GiB | ✓ Paris | ✗  (thought first) |
| Qwen3.5 2B | Cpu (-1 layers) | 208.4 | 233.5 | 0.5 s | 2.2 GiB | 1.4 GiB | ✓ Paris | ✗  (thought first) |
| LFM2.5 2.6B | – | – | – | – | – | – | FAILED: the model stopped unexpectedly: the model's chat template could not be used: syntax error: unknown statement generation (in chat:86) | |
| Qwen3.5 4B | Cpu (-1 layers) | 112.4 | 128.3 | 0.1 s | 3.9 GiB | 2.8 GiB | ✓ Paris | ✓ 391 (thought first) |
| Gemma 4 E2B | Cpu (-1 layers) | 153.9 | 176.6 | 0.5 s | 3.8 GiB | 1.7 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.5 9B | Cpu (-1 layers) | 73.2 | 84.4 | 0.2 s | 6.6 GiB | 4.9 GiB | ✓ Paris | ✓ 391 (thought first) |
| Gemma 4 12B | Cpu (-1 layers) | 49.1 | 53.7 | 0.5 s | 8.1 GiB | 7.2 GiB | ✓ Paris | ✓ 391 (thought first) |
| gpt-oss 20B | Cpu (-1 layers) | 38.2 | 49.9 | 0.6 s | 12.4 GiB | 12.8 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.6 35B | Gpu (41 layers) | 17.7 | 19.6 | 7.8 s | 20.1 GiB | 31.0 GiB | ✓ Paris | ✓ 391 (thought first) |
