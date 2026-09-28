(() => {
  const STYLE_ID = "system-ui-css";

  function applySystemUiCss(css) {
    let style = document.getElementById(STYLE_ID);
    const text = String(css || "");
    if (!style) {
      if (!text) {
        return;
      }
      style = document.createElement("style");
      style.id = STYLE_ID;
      document.head.appendChild(style);
    }
    style.textContent = text;
  }

  function applyFromBootstrap(bootstrap) {
    applySystemUiCss(bootstrap?.system_ui_css ?? bootstrap?.systemUiCss ?? "");
  }

  function bindEscapeToClose(invoke) {
    window.addEventListener("keydown", (event) => {
      if (event.key !== "Escape" && event.key !== "Esc") {
        return;
      }
      if (event.defaultPrevented || event.isComposing || event.repeat) {
        return;
      }
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }
      event.preventDefault();
      invoke("close_system_window").catch(() => {});
    });
  }

  window.StatTrackerSystemWindow = {
    applyFromBootstrap,
    bindEscapeToClose,
  };
})();
