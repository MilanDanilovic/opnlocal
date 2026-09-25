// Two things run in the browser, both without sending anything anywhere:
// 1. the download button matches the visitor's platform;
// 2. the device card shows the little that a browser can tell (system, processor threads).
(function () {
  const RELEASES = "https://github.com/MilanDanilovic/opnlocal/releases/latest";
  const file = (name) => `${RELEASES}/download/${name}`;
  const ua = navigator.userAgent;
  const platform = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || "";
  const touchMac = platform === "MacIntel" && navigator.maxTouchPoints > 1; // iPadOS reports as a Mac

  let system, download;
  if (/Android/i.test(ua)) {
    system = "Android";
    download = { label: "Download for Android", href: file("opnlocal-android.apk") };
  } else if (/iPhone|iPad|iPod/i.test(ua) || touchMac) {
    system = /iPad/i.test(ua) || touchMac ? "iPadOS" : "iOS";
    download = { label: "View all downloads", href: RELEASES, other: "The iPhone build needs sideloading; see the release page." };
  } else if (/Windows/i.test(ua)) {
    system = "Windows";
    download = { label: "Download for Windows", href: file("opnlocal-windows-setup.exe") };
  } else if (/Mac/i.test(platform) || /Macintosh/i.test(ua)) {
    system = "macOS";
    download = { label: "Download for Mac", href: file("opnlocal-macos.dmg"), other: "Apple silicon (M1 or newer)." };
  } else if (/Linux|X11/i.test(platform) || /Linux/i.test(ua)) {
    system = "Linux";
    download = { label: "Download for Linux (.deb)", href: file("opnlocal-linux.deb"), other: `or the <a href="${file("opnlocal-linux.AppImage")}">AppImage</a> for other distributions.` };
  } else {
    system = "Unknown";
    download = { label: "View all downloads", href: RELEASES };
  }

  const btn = document.getElementById("download");
  btn.textContent = download.label;
  btn.href = download.href;
  if (download.other) {
    const note = document.createElement("span");
    note.className = "fine";
    note.innerHTML = " " + download.other;
    btn.parentElement.insertAdjacentElement("afterend", note);
  }

  const cores = navigator.hardwareConcurrency;
  document.getElementById("d-os").textContent = system === "Unknown" ? "Not reported by this browser" : system;
  document.getElementById("d-cores").textContent = cores ? String(cores) : "Not reported by this browser";
})();
