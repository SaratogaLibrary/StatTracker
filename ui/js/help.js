(() => {
  const list = document.getElementById("help-list");
  const status = document.getElementById("help-status");
  const ATTRS = ["title", "aria-label", "placeholder", "alt"];

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  }

  function fillPlaceholders(source, config) {
    const windowTitle = config.window_title || "StatTracker";
    const deskName = config.desk_name || "this desk";
    const orgName = config.org_name || "";
    return String(source)
      .replaceAll("{{windowTitle}}", windowTitle)
      .replaceAll("{{deskName}}", deskName)
      .replaceAll("{{orgName}}", orgName);
  }

  function applyPlaceholders(root, config) {
    const visit = (node) => {
      if (node.nodeType === Node.TEXT_NODE) {
        const raw = node.__stTpl ?? node.nodeValue;
        if (raw && raw.includes("{{")) {
          node.__stTpl = raw;
          node.nodeValue = fillPlaceholders(raw, config);
        }
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) {
        return;
      }
      ATTRS.forEach((attr) => {
        if (!node.hasAttribute(attr)) {
          return;
        }
        const key = `__stTpl_${attr}`;
        const raw = node[key] ?? node.getAttribute(attr);
        if (raw && raw.includes("{{")) {
          node[key] = raw;
          node.setAttribute(attr, fillPlaceholders(raw, config));
        }
      });
      node.childNodes.forEach(visit);
    };
    visit(root);
  }

  async function whenTauriReady() {
    for (let attempt = 0; attempt < 80; attempt += 1) {
      const api = window.__TAURI__;
      if (api?.tauri?.invoke) {
        return api;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 50));
    }
    throw new Error("The help window failed to connect to the application.");
  }

  function render(bootstrap, api) {
    const config = bootstrap?.config || {};
    applyPlaceholders(document.documentElement, config);
    const heading = fillPlaceholders("{{windowTitle}} Info", config);
    document.title = heading;
    const currentWindow = api?.window?.getCurrent?.() || api?.window?.appWindow;
    if (currentWindow?.setTitle) {
      currentWindow.setTitle(heading).catch(() => {});
    }
    const types = bootstrap?.question_types || [];
    list.replaceChildren();
    if (!types.length) {
      status.hidden = false;
      status.textContent =
        bootstrap?.last_error ||
        "No question types are cached for this desk yet. Open Settings, save a desk, and try again.";
      return;
    }
    status.hidden = true;
    status.textContent = "";
    types.forEach((questionType) => {
      const item = document.createElement("li");
      item.className = "help-item";
      const name = escapeHtml(questionType.name || "Untitled");
      const description = String(questionType.description || "").trim();
      item.innerHTML = description
        ? `<h2>${name}</h2><p>${escapeHtml(description)}</p>`
        : `<h2>${name}</h2><p class="is-empty">No description provided.</p>`;
      list.appendChild(item);
    });
  }

  async function start() {
    const api = await whenTauriReady();
    const invoke = api.tauri.invoke;
    await api.event.listen("config-saved", (event) => {
      render(event.payload || {}, api);
    });
    render(await invoke("get_bootstrap"), api);
  }

  window.addEventListener("DOMContentLoaded", () => {
    start().catch((error) => {
      status.hidden = false;
      status.textContent = String(error);
    });
  });
})();
