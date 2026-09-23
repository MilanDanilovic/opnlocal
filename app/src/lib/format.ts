// Plain-language formatting. Sizes use decimal units (1 GB = 1,000,000,000 bytes), the same
// units phones and operating systems show in their storage settings.

/** "2.7 GB", "830 MB", "12 KB". */
export function bytes(n: number): string {
  if (n >= 1e9) return `${trim1(n / 1e9)} GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)} MB`;
  if (n >= 1e3) return `${Math.round(n / 1e3)} KB`;
  return `${n} bytes`;
}

// Memory is sold and quoted in binary sizes ("16 GB" RAM = 16 GiB), so memory uses those.
const GIB = 2 ** 30;
const COMMON_SIZES = [1, 2, 3, 4, 6, 8, 10, 12, 16, 18, 20, 24, 32, 36, 48, 64, 96, 128, 192, 256];

/** Installed memory as people know it: the OS reports 31.1 GiB of a 32 GB machine → "32 GB". */
export function memory(n: number): string {
  const gib = n / GIB;
  const size = COMMON_SIZES.find((s) => gib <= s * 1.02 && gib >= s * 0.9);
  if (size) return `${size} GB`;
  return `${trim1(gib)} GB`;
}

/** A memory amount (shortfall, need), rounded up to 0.1 GB in the same binary units. */
export function memoryUp(n: number): string {
  return `${trim1(Math.ceil((n / GIB) * 10) / 10)} GB`;
}

/** Rounds up to one decimal: space someone must free should never be understated. */
export function bytesUp(n: number): string {
  if (n >= 1e9) return `${trim1(Math.ceil(n / 1e8) / 10)} GB`;
  return bytes(n);
}

function trim1(x: number): string {
  const s = x.toFixed(1);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
}

/** "about 3 minutes", "less than a minute", "about 1 hour 10 minutes". */
export function duration(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return "";
  if (seconds < 60) return "less than a minute";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `about ${minutes} minute${minutes === 1 ? "" : "s"}`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return `about ${h} hour${h === 1 ? "" : "s"}${m ? ` ${m} minute${m === 1 ? "" : "s"}` : ""}`;
}

/** "1.2 seconds", "450 milliseconds" -> shown as "0.5 seconds" for readability. */
export function seconds(ms: number): string {
  const s = ms / 1000;
  if (s < 10) return `${s.toFixed(1)} seconds`;
  return `${Math.round(s)} seconds`;
}

/** Measured speed in words per second, rounded for humans ("about 12"). */
export function wordsPerSecond(wps: number): string {
  if (wps >= 10) return `${Math.round(wps)}`;
  return wps.toFixed(1).replace(/\.0$/, "");
}

export function percent(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.floor((done / total) * 100));
}

export function countWords(text: string): number {
  return text.split(/\s+/).filter((w) => /[\p{L}\p{N}]/u.test(w)).length;
}

export function number(n: number): string {
  return new Intl.NumberFormat("en").format(n);
}

/** "Today", "Yesterday", or a short date, for conversation lists. */
export function day(unixSeconds: number, now = Date.now()): string {
  const d = new Date(unixSeconds * 1000);
  const today = new Date(now);
  const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((startOf(today) - startOf(d)) / 86_400_000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return d.toLocaleDateString("en", { day: "numeric", month: "short", year: diff > 300 ? "numeric" : undefined });
}
