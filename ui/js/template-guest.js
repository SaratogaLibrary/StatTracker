(() => {
  try {
    Object.defineProperty(window, "__TAURI__", {
      configurable: false,
      enumerable: false,
      get() {
        return undefined;
      },
      set() {},
    });
  } catch {
    try {
      window.__TAURI__ = undefined;
    } catch {
      /* already locked */
    }
  }

  const SOURCE = "stattracker-template";
  const HOST = "stattracker-host";
  const FADE_MS = 800;
  let requestId = 0;
  const pending = new Map();
  let lastSize = { width: 0, height: 0 };
  let queuedBootstrap = null;
  let documentReady = false;

  function post(payload) {
    parent.postMessage({ source: SOURCE, ...payload }, "*");
  }

  function escapeHtml(value) {
    return String(value)
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  }

  const StatTracker = {
    config: null,
    questionTypes: [],
    stats: { last_hour: 0, today: 0, week: 0, month: 0, year: 0 },
    online: false,
    lastError: null,
    recordTally(questionTypeId) {
      const id = ++requestId;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        post({
          type: "recordTally",
          requestId: id,
          questionTypeId: Number(questionTypeId),
        });
      });
    },
    refresh() {
      post({ type: "action", action: "refresh" });
    },
    cycleTemplate() {
      post({ type: "action", action: "cycle-template" });
    },
    openSettings() {
      post({ type: "action", action: "settings" });
    },
    openHelp() {
      post({ type: "action", action: "help" });
    },
  };
  ["recordTally", "refresh", "cycleTemplate", "openSettings", "openHelp"].forEach((name) => {
    Object.defineProperty(StatTracker, name, {
      writable: false,
      configurable: false,
    });
  });
  window.StatTracker = StatTracker;

  function updateSlots(bootstrap) {
    const setText = (slot, value) => {
      document.querySelectorAll(`[data-slot="${slot}"]`).forEach((node) => {
        node.textContent = value;
      });
    };
    setText("counter", String(bootstrap.stats?.today ?? 0));
    setText("status", bootstrap.online ? "Online" : "Offline");
    setText("desk-name", bootstrap.config?.desk_name || "");
    setText("org-name", bootstrap.config?.org_name || "");
    setText("title", bootstrap.config?.window_title || "StatTracker");
    document.querySelectorAll('[data-slot="status"]').forEach((node) => {
      node.dataset.online = bootstrap.online ? "true" : "false";
    });
    pinButtons(Boolean(bootstrap.config?.always_on_top ?? bootstrap.config?.alwaysOnTop));
  }

  function pinButtons(alwaysOnTop) {
    document.querySelectorAll('[data-action="always-on-top"]').forEach((node) => {
      node.setAttribute("aria-pressed", alwaysOnTop ? "true" : "false");
      node.classList.toggle("is-active", alwaysOnTop);
    });
  }

  function createTallyButton(questionType) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "tally";
    button.dataset.questionTypeId = String(questionType.id);
    const description = questionType.description || questionType.name;
    button.title = description;
    button.setAttribute("aria-label", description);
    button.innerHTML = `<span>${escapeHtml(questionType.name)}</span>`;
    button.addEventListener("click", async () => {
      if (button.disabled) {
        return;
      }
      button.classList.add("is-pending");
      try {
        const result = await StatTracker.recordTally(questionType.id);
        StatTracker.stats = result.stats;
        updateSlots({
          stats: result.stats,
          online: StatTracker.online,
          config: StatTracker.config,
        });
      } catch {
        /* The host shows the error dialog. */
      } finally {
        button.classList.remove("is-pending");
        button.classList.add("is-flash");
        window.setTimeout(() => button.classList.remove("is-flash"), FADE_MS);
      }
    });
    return button;
  }

  function fillButtons(questionTypes) {
    const slot = document.querySelector('[data-slot="buttons"]');
    if (!slot) {
      return;
    }
    slot.replaceChildren();
    questionTypes.forEach((questionType) => {
      slot.appendChild(createTallyButton(questionType));
    });
  }

  async function waitForLayout() {
    if (document.fonts) {
      await document.fonts.ready.catch(() => {});
    }
    await new Promise((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(resolve));
    });
  }

  function measure() {
    const widget = document.querySelector("[data-widget]") || document.body;
    const rect = widget.getBoundingClientRect();
    return {
      width: Math.ceil(Math.max(widget.scrollWidth, rect.width, 64)),
      height: Math.ceil(Math.max(widget.scrollHeight, rect.height, 48)),
    };
  }

  function reportSize() {
    const size = measure();
    if (size.width === lastSize.width && size.height === lastSize.height) {
      return;
    }
    lastSize = size;
    post({ type: "resize", width: size.width, height: size.height });
  }

  function questionTypeIds(list) {
    return (list || []).map((item) => String(item.id)).join(",");
  }

  async function applyBootstrap(payload, isInitial) {
    StatTracker.config = payload.config || StatTracker.config;
    StatTracker.stats = payload.stats || StatTracker.stats;
    StatTracker.online = payload.online;
    StatTracker.lastError = payload.lastError ?? payload.last_error ?? null;
    const nextTypes = payload.questionTypes || payload.question_types;
    const typesChanged =
      Array.isArray(nextTypes)
      && questionTypeIds(nextTypes) !== questionTypeIds(StatTracker.questionTypes);
    if (Array.isArray(nextTypes)) {
      StatTracker.questionTypes = nextTypes;
    }
    if ((isInitial || typesChanged) && StatTracker.questionTypes.length) {
      fillButtons(StatTracker.questionTypes);
    }
    updateSlots({
      stats: StatTracker.stats,
      online: StatTracker.online,
      config: StatTracker.config,
    });
    if (isInitial) {
      document.dispatchEvent(
        new CustomEvent("stattracker:ready", { detail: payload }),
      );
    }
    if (isInitial || typesChanged) {
      await waitForLayout();
      lastSize = { width: 0, height: 0 };
      reportSize();
    }
  }

  window.addEventListener("message", (event) => {
    if (event.source !== parent) {
      return;
    }
    const data = event.data;
    if (!data || data.source !== HOST) {
      return;
    }
    if (data.type === "bootstrap") {
      const payload = data.payload || {};
      if (!documentReady) {
        queuedBootstrap = payload;
        return;
      }
      applyBootstrap(payload, true).catch(() => {});
      return;
    }
    if (data.type === "update") {
      applyBootstrap(data.payload || {}, false).catch(() => {});
      return;
    }
    if (data.type === "appearance") {
      const alwaysOnTop = Boolean(
        data.payload?.alwaysOnTop ?? data.payload?.always_on_top,
      );
      if (StatTracker.config) {
        StatTracker.config.always_on_top = alwaysOnTop;
      }
      pinButtons(alwaysOnTop);
      return;
    }
    if (data.type === "tallyResult") {
      const waiter = pending.get(data.requestId);
      if (!waiter) {
        return;
      }
      pending.delete(data.requestId);
      if (data.ok) {
        waiter.resolve({ stats: data.stats, online: data.online });
      } else {
        waiter.reject(new Error(data.error || "The tally could not be saved."));
      }
    }
  });

  document.addEventListener("click", (event) => {
    const node = event.target.closest("[data-action]");
    if (!node) {
      return;
    }
    event.preventDefault();
    const action = node.getAttribute("data-action");
    if (action) {
      post({ type: "action", action });
    }
  });

  document.addEventListener("mousedown", (event) => {
    if (event.button !== 0) {
      return;
    }
    if (!event.target.closest("[data-tauri-drag-region]")) {
      return;
    }
    event.preventDefault();
    post({ type: "dragStart" });
  });

  window.addEventListener("DOMContentLoaded", () => {
    documentReady = true;
    post({ type: "ready" });
    if (queuedBootstrap) {
      const payload = queuedBootstrap;
      queuedBootstrap = null;
      applyBootstrap(payload, true).catch(() => {});
    }
  });

  if (document.readyState !== "loading") {
    documentReady = true;
    post({ type: "ready" });
  }
})();
