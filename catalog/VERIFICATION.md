# Catalog verification

Catalog version 2. Run on Windows 11 Pro, AMD Ryzen 7 7700X 8-Core Processor (31 GiB RAM), AMD Radeon RX 7800 XT (15 GiB), AMD Radeon(TM) Graphics (15 GiB).

Every model was downloaded from its pinned Hugging Face commit, checked against its sha256, loaded by the app's engine, benchmarked, and asked two questions (thinking off, then "Think harder").

*App RAM + GPU* is the process's resident memory plus the drop in free graphics memory, as reported by the OS and driver. It over-counts when a model sits on the GPU: the memory-mapped model file still shows as resident (the OS can reclaim it), and drivers release memory lazily. The estimate is what the app uses to decide fit.

| Model | Placement | Words/s | Tokens/s | First word | Estimated need | App RAM + GPU | Plain answer | Think-harder answer |
|---|---|---|---|---|---|---|---|---|
| Qwen3.5 0.8B | Gpu (24 layers) | 267.4 | 309.5 | 0.0 s | 1.8 GiB | 2.4 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.5 2B | Gpu (24 layers) | 212.2 | 237.8 | 0.1 s | 2.2 GiB | 2.3 GiB | ✓ Paris | ✓ 391 (thought first) |
| LFM2.5 2.6B | Gpu (30 layers) | 164.1 | 228.8 | 0.1 s | 2.4 GiB | 2.4 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.5 4B | Gpu (32 layers) | 112.2 | 128.0 | 0.1 s | 3.9 GiB | 4.6 GiB | ✓ Paris | ✓ 391 (thought first) |
| Gemma 4 E2B | Gpu (35 layers) | 153.7 | 176.4 | 0.1 s | 3.8 GiB | 1.6 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.5 9B | Gpu (32 layers) | 73.2 | 84.4 | 0.2 s | 6.6 GiB | 9.1 GiB | ✓ Paris | ✓ 391 (thought first) |
| Gemma 4 12B | Gpu (48 layers) | 48.8 | 53.4 | 0.3 s | 8.1 GiB | 19.4 GiB | ✓ Paris | ✓ 391 (thought first) |
| gpt-oss 20B | Gpu (24 layers) | 32.7 | 43.1 | 0.7 s | 12.4 GiB | 16.3 GiB | ✓ Paris | ✓ 391 (thought first) |
| Qwen3.6 35B | Gpu (40 layers) | 17.2 | 19.1 | 13.4 s | 20.1 GiB | 19.8 GiB | ✓ Paris | ✓ 391 (thought first) |
