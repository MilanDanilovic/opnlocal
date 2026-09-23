// Moves keyboard/screen-reader focus to a screen's heading when it appears, so each screen
// change is announced like a page load.
export function focusOnMount(node: HTMLElement) {
  node.setAttribute("tabindex", "-1");
  requestAnimationFrame(() => node.focus({ preventScroll: true }));
}
