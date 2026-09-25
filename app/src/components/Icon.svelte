<script lang="ts" module>
  // 24×24 stroke icons. Decorative by default (aria-hidden); give the button a label instead.
  const paths: Record<string, string> = {
    menu: "M4 6h16M4 12h16M4 18h16",
    plus: "M12 5v14M5 12h14",
    send: "M5 12h13M13 6l6 6-6 6",
    stop: "M7 7h10v10H7z",
    attach: "M16.5 6.5l-7.4 7.4a2 2 0 002.8 2.8l7.4-7.4a4 4 0 00-5.7-5.7L6.2 11a6 6 0 008.5 8.5l5.3-5.3",
    copy: "M9 9h10v10H9zM5 15V5h10",
    check: "M5 12.5l4.5 4.5L19 7.5",
    refresh: "M20 11a8 8 0 10-2.3 5.7M20 5v6h-6",
    settings: "M12 15a3 3 0 100-6 3 3 0 000 6zM19.4 13a7.5 7.5 0 000-2l2-1.5-2-3.4-2.4.9a7.6 7.6 0 00-1.7-1L15 3.5h-4l-.4 2.5a7.6 7.6 0 00-1.7 1l-2.4-.9-2 3.4 2 1.5a7.5 7.5 0 000 2l-2 1.5 2 3.4 2.4-.9c.5.4 1.1.7 1.7 1l.4 2.5h4l.4-2.5c.6-.3 1.2-.6 1.7-1l2.4.9 2-3.4z",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 12h10l1-12M9 7V4h6v3",
    back: "M15 5l-7 7 7 7",
    down: "M6 9l6 6 6-6",
    chip: "M8 8h8v8H8zM4 10h2M4 14h2M18 10h2M18 14h2M10 4v2M14 4v2M10 18v2M14 18v2",
    memory: "M3 8h18v8H3zM7 8v8M11 8v8M15 8v8M5 16v3M19 16v3",
    disk: "M4 6h16v12H4zM4 14h16M16 16.5h.01",
    gpu: "M3 7h18v10H3zM7 17v3M17 17v3M8 12a2 2 0 104 0 2 2 0 00-4 0zM15 11h3M15 13h3",
    shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z",
    lock: "M6 11h12v9H6zM9 11V8a3 3 0 016 0v3",
    bolt: "M13 3L5 14h6l-1 7 8-11h-6z",
    x: "M6 6l12 12M18 6L6 18",
    info: "M12 11v6M12 7.5v.01M12 21a9 9 0 100-18 9 9 0 000 18z",
    download: "M12 4v11M7 10l5 5 5-5M5 20h14",
    think: "M9 18h6M10 21h4M12 3a6 6 0 00-3.5 10.9c.6.5 1 1.2 1 2.1h5c0-.9.4-1.6 1-2.1A6 6 0 0012 3z",
    chat: "M5 5h14v10H9l-4 4z",
    doc: "M7 3h7l4 4v14H7zM14 3v4h4M10 12h5M10 16h5",
    image: "M4 5h16v14H4zM4 15l5-5 4 4 3-3 4 4M15 9h.01",
    code: "M9 8l-4 4 4 4M15 8l4 4-4 4",
    pen: "M4 20l4-1 11-11-3-3L5 16zM14 6l3 3",
    sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7z",
    external: "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
    wifiOff: "M3 3l18 18M8.5 16.5a5 5 0 017 0M5 12.5a10 10 0 015.2-2.8M19 12.5a10 10 0 00-3-2.2M2 8.8A15 15 0 016.7 6M22 8.8A15 15 0 0011 5M12 20h.01",
  };
</script>

<script lang="ts">
  let { name, size = 20, label }: { name: keyof typeof paths | string; size?: number; label?: string } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden={label ? undefined : "true"}
  role={label ? "img" : undefined}
  aria-label={label}
  focusable="false"
>
  <path d={paths[name] ?? paths.info} />
</svg>
