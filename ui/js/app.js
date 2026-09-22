(() => {
  const invoke = window.__TAURI__.tauri.invoke;
  const listen = window.__TAURI__.event.listen;
  const dialog = window.__TAURI__.dialog;
  const appWindow = window.__TAURI__.window.appWindow;
  const host = document.getElementById("host");

  const FADE_MS = 800;
  const TEMPLATE_SOURCE = "stattracker-template";
  const HOST_SOURCE = "stattracker-host";
  const ALLOWED_MESSAGES = new Set([
    "ready",
    "action",
    "recordTally",
    "resize",
    "dragStart",
  ]);
  const ALLOWED_ACTIONS = new Set([
    "close",
    "minimize",
    "settings",
    "help",
    "refresh",
    "cycle-template",
    "always-on-top",
  ]);

  const session = {
    zoom: 1,
    baseSize: { width: 480, height: 88 },
    alwaysOnTop: false,
    unfocusedOpacity: 55,
    inactivityMs: 0,
    inactivityMessage: "",
    inactivityTimer: null,
    hovered: false,
    placedOnStartup: false,
    frame: null,
    frameReady: false,
    pendingBootstrap: null,
    guestScript: null,
  };

  const sleep = (ms) => new Promise((resolve) => window.setTimeout(resolve, ms));

  function escapeClosingTag(value, tag) {
    const pattern = new RegExp(`</${tag}`, "gi");
    return String(value).replace(pattern, `<\\/${tag}`);
  }

  async function loadGuestScript() {
    if (session.guestScript != null) {
      return session.guestScript;
    }
    const response = await fetch("js/template-guest.js");
    if (!response.ok) {
      throw new Error("Could not load the template guest script.");
    }
    session.guestScript = await response.text();
    return session.guestScript;
  }

  function extractBodyHtml(html) {
    const parsed = new DOMParser().parseFromString(html || "", "text/html");
    parsed.body.querySelectorAll("script").forEach((node) => node.remove());
    return parsed.body.innerHTML;
  }

  function buildSrcdoc(template, guestJs) {
    const css = escapeClosingTag(template.css || "", "style");
    const body = extractBodyHtml(template.html);
    const js = escapeClosingTag(template.js || "", "script");
    const guest = escapeClosingTag(guestJs, "script");
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <style>
    html, body {
      margin: 0;
      padding: 0;
      background: transparent;
      overflow: hidden;
      width: max-content;
      height: max-content;
      max-width: none;
      max-height: none;
    }
    ${css}
    [data-slot="buttons"] {
      flex-grow: 0;
      flex-shrink: 0;
      flex-basis: auto;
    }
  </style>
  <script>${guest}<\/script>
</head>
<body>
${body}
<script>${js}<\/script>
</body>
</html>`;
  }

  function postToFrame(message) {
    const frame = session.frame;
    if (!frame?.contentWindow) {
      return;
    }
    frame.contentWindow.postMessage({ source: HOST_SOURCE, ...message }, "*");
  }

  function sendBootstrap(bootstrap) {
    const payload = {
      config: bootstrap.config,
      questionTypes: bootstrap.question_types || bootstrap.questionTypes || [],
      stats: bootstrap.stats,
      online: bootstrap.online,
      lastError: bootstrap.last_error ?? bootstrap.lastError ?? null,
    };
    if (!session.frameReady) {
      session.pendingBootstrap = payload;
      return;
    }
    postToFrame({ type: "bootstrap", payload });
  }

  function sendUpdate() {
    if (!window.StatTracker.config) {
      return;
    }
    postToFrame({
      type: "update",
      payload: {
        config: window.StatTracker.config,
        questionTypes: window.StatTracker.questionTypes,
        stats: window.StatTracker.stats,
        online: window.StatTracker.online,
        lastError: window.StatTracker.lastError,
      },
    });
  }

  function applyZoomStyle() {
    host.style.zoom = String(session.zoom);
    host.style.setProperty("--widget-zoom", String(session.zoom));
    host.style.removeProperty("transform");
    host.style.removeProperty("transform-origin");
  }

  async function applyZoom() {
    applyZoomStyle();
    const width = session.baseSize.width * session.zoom;
    const height = session.baseSize.height * session.zoom;
    const prior = await invoke("widget_outer_rect").catch(() => null);
    await invoke("place_widget", {
      width,
      height,
      dock: !session.placedOnStartup,
      prior,
    }).catch(() => invoke("set_widget_size", { width, height }));
  }

  async function applyOpacity() {
    const dim =
      session.alwaysOnTop && !session.hovered
        ? Math.max(0, Math.min(100, session.unfocusedOpacity)) / 100
        : 1;
    host.style.setProperty("--dim", String(dim));
  }

  async function applyAppearance(config, options = {}) {
    if (!config) {
      return;
    }
    const opacity = Number(config.unfocused_opacity ?? config.unfocusedOpacity);
    if (Number.isFinite(opacity)) {
      session.unfocusedOpacity = Math.max(0, Math.min(100, opacity));
    }
    if (typeof config.always_on_top === "boolean" || typeof config.alwaysOnTop === "boolean") {
      session.alwaysOnTop = Boolean(config.always_on_top ?? config.alwaysOnTop);
      if (options.persistAlwaysOnTop !== false) {
        await invoke("set_always_on_top", { enabled: session.alwaysOnTop }).catch(() => {});
      }
    }
    if (Object.hasOwn(config, "inactivity_enabled")
      || Object.hasOwn(config, "inactivityEnabled")) {
      session.inactivityMessage = config.inactivity_message || config.inactivityMessage || "";
      const enabled = Boolean(config.inactivity_enabled ?? config.inactivityEnabled);
      const minutes = Number(config.inactivity_minutes ?? config.inactivityMinutes) || 15;
      session.inactivityMs = enabled ? Math.max(1, minutes) * 60 * 1000 : 0;
      resetInactivity();
    }
    postToFrame({
      type: "appearance",
      payload: { alwaysOnTop: session.alwaysOnTop },
    });
    await applyOpacity();
  }

  function widgetNeedsRebuild(nextConfig) {
    const prev = window.StatTracker.config;
    if (!prev || !session.frame) {
      return true;
    }
    return prev.active_template !== nextConfig.active_template
      || Number(prev.desk_id) !== Number(nextConfig.desk_id)
      || prev.base_url !== nextConfig.base_url
      || prev.storage_directory !== nextConfig.storage_directory;
  }

  function resetInactivity() {
    window.clearTimeout(session.inactivityTimer);
    if (!session.inactivityMs) {
      return;
    }
    session.inactivityTimer = window.setTimeout(async () => {
      await dialog.message(session.inactivityMessage, {
        title: "StatTracker",
        type: "info",
      });
      resetInactivity();
    }, session.inactivityMs);
  }

  async function handleResize(width, height) {
    const nextWidth = Math.min(2000, Math.max(64, Math.ceil(Number(width) || 0)));
    const nextHeight = Math.min(2000, Math.max(48, Math.ceil(Number(height) || 0)));
    const same =
      nextWidth === session.baseSize.width
      && nextHeight === session.baseSize.height
      && session.placedOnStartup;
    session.baseSize = { width: nextWidth, height: nextHeight };
    if (session.frame) {
      session.frame.style.width = `${nextWidth}px`;
      session.frame.style.height = `${nextHeight}px`;
    }
    if (same) {
      return;
    }
    applyZoomStyle();
    const scaledWidth = nextWidth * session.zoom;
    const scaledHeight = nextHeight * session.zoom;
    const prior = await invoke("widget_outer_rect").catch(() => null);
    await invoke("place_widget", {
      width: scaledWidth,
      height: scaledHeight,
      dock: !session.placedOnStartup,
      prior,
    }).catch(() => invoke("set_widget_size", { width: scaledWidth, height: scaledHeight }));
    session.placedOnStartup = true;
    await invoke("show_widget");
    requestAnimationFrame(() => host.classList.add("is-visible"));
    await applyOpacity();
  }

  async function handleTemplateAction(action) {
    switch (action) {
      case "close":
        host.classList.remove("is-visible");
        await sleep(FADE_MS);
        await invoke("close_widget");
        break;
      case "minimize":
        await invoke("minimize_widget");
        break;
      case "settings":
        await invoke("open_settings");
        break;
      case "help":
        await invoke("open_help");
        break;
      case "refresh":
        await render(true);
        break;
      case "cycle-template":
        await invoke("cycle_template");
        break;
      case "always-on-top":
        session.alwaysOnTop = !session.alwaysOnTop;
        await invoke("set_always_on_top", { enabled: session.alwaysOnTop });
        postToFrame({
          type: "appearance",
          payload: { alwaysOnTop: session.alwaysOnTop },
        });
        await applyOpacity();
        break;
      default:
        break;
    }
  }

  async function handleRecordTally(requestId, questionTypeId) {
    const id = Number(questionTypeId);
    if (!Number.isFinite(id) || !Number.isFinite(Number(requestId))) {
      return;
    }
    try {
      const result = await window.StatTracker.recordTally(id);
      window.StatTracker.stats = result.stats;
      if (typeof result.online === "boolean") {
        window.StatTracker.online = result.online;
      }
      resetInactivity();
      postToFrame({
        type: "tallyResult",
        requestId,
        ok: true,
        stats: result.stats,
        online: window.StatTracker.online,
      });
    } catch (error) {
      postToFrame({
        type: "tallyResult",
        requestId,
        ok: false,
        error: String(error),
      });
      await dialog.message(String(error), { title: "StatTracker", type: "error" });
    }
  }

  function onFrameMessage(event) {
    if (!session.frame || event.source !== session.frame.contentWindow) {
      return;
    }
    const data = event.data;
    if (!data || data.source !== TEMPLATE_SOURCE || !ALLOWED_MESSAGES.has(data.type)) {
      return;
    }
    if (data.type === "ready") {
      session.frameReady = true;
      if (session.pendingBootstrap) {
        postToFrame({ type: "bootstrap", payload: session.pendingBootstrap });
        session.pendingBootstrap = null;
      }
      return;
    }
    if (data.type === "action") {
      if (!ALLOWED_ACTIONS.has(data.action)) {
        return;
      }
      handleTemplateAction(data.action).catch((error) => {
        dialog.message(String(error), { title: "StatTracker", type: "error" });
      });
      return;
    }
    if (data.type === "recordTally") {
      handleRecordTally(data.requestId, data.questionTypeId).catch(() => {});
      return;
    }
    if (data.type === "resize") {
      handleResize(data.width, data.height).catch(() => {});
      return;
    }
    if (data.type === "dragStart") {
      appWindow.startDragging().catch(() => {});
    }
  }

  async function mountTemplate(template, bootstrap) {
    const guestJs = await loadGuestScript();
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", "allow-scripts");
    frame.setAttribute("allowtransparency", "true");
    frame.title = bootstrap.config?.window_title || "StatTracker";
    frame.style.border = "0";
    frame.style.display = "block";
    frame.style.background = "transparent";
    frame.style.width = "900px";
    frame.style.height = "2400px";
    session.frame = frame;
    session.frameReady = false;
    session.pendingBootstrap = null;
    session.baseSize = { width: 0, height: 0 };
    host.replaceChildren(frame);
    frame.srcdoc = buildSrcdoc(template, guestJs);
    sendBootstrap(bootstrap);
  }

  let rendering = false;

  async function restoreVisible() {
    host.classList.add("is-visible");
    await invoke("show_widget").catch(() => {});
    await applyOpacity();
  }

  async function render(refresh) {
    if (rendering) {
      return;
    }
    rendering = true;
    const wasVisible = host.classList.contains("is-visible");
    try {
      const bootstrap = refresh
        ? await invoke("refresh_question_types")
        : await invoke("get_bootstrap");

      if (!bootstrap.configured || !bootstrap.question_types.length) {
        if (wasVisible) {
          await restoreVisible();
        }
        await invoke("open_settings");
        return;
      }

      if (wasVisible) {
        host.classList.remove("is-visible");
        await sleep(FADE_MS);
      }

      window.StatTracker.config = bootstrap.config;
      window.StatTracker.questionTypes = bootstrap.question_types;
      window.StatTracker.stats = bootstrap.stats;
      window.StatTracker.online = bootstrap.online;
      window.StatTracker.lastError = bootstrap.last_error;

      await applyAppearance(bootstrap.config);

      const template = bootstrap.template;
      if (!template) {
        if (wasVisible) {
          await restoreVisible();
        }
        return;
      }

      document.title = bootstrap.config.window_title || "StatTracker";
      resetInactivity();
      await mountTemplate(template, bootstrap);
    } catch (error) {
      if (wasVisible) {
        await restoreVisible();
      } else {
        await invoke("open_settings").catch(() => {});
      }
      throw error;
    } finally {
      rendering = false;
    }
  }

  host.addEventListener("mouseenter", () => {
    session.hovered = true;
    applyOpacity();
  });
  host.addEventListener("mouseleave", () => {
    session.hovered = false;
    applyOpacity();
  });

  document.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey)) {
      return;
    }
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      session.zoom = Math.min(1.8, Math.round((session.zoom + 0.1) * 10) / 10);
      applyZoom();
    } else if (event.key === "-") {
      event.preventDefault();
      session.zoom = Math.max(0.7, Math.round((session.zoom - 0.1) * 10) / 10);
      applyZoom();
    } else if (event.key === "0") {
      event.preventDefault();
      session.zoom = 1;
      applyZoom();
    }
  });

  window.addEventListener("message", onFrameMessage);

  async function boot() {
    await listen("config-saved", async (event) => {
      const bootstrap = event.payload;
      if (!bootstrap?.config) {
        await render(false);
        return;
      }
      const needsRebuild = widgetNeedsRebuild(bootstrap.config) || !bootstrap.question_types?.length;
      window.StatTracker.config = bootstrap.config;
      window.StatTracker.stats = bootstrap.stats || window.StatTracker.stats;
      window.StatTracker.online = bootstrap.online;
      window.StatTracker.lastError = bootstrap.last_error;
      if (bootstrap.question_types?.length) {
        window.StatTracker.questionTypes = bootstrap.question_types;
      }
      await applyAppearance(bootstrap.config);
      if (needsRebuild) {
        await render(false);
      } else if (session.frame) {
        sendUpdate();
      }
    });
    await listen("appearance-preview", async (event) => {
      await applyAppearance(event.payload || {}, { persistAlwaysOnTop: false });
    });
    await listen("connectivity", (event) => {
      window.StatTracker.online = event.payload.online;
      window.StatTracker.lastError = event.payload.last_error;
      if (window.StatTracker.config) {
        sendUpdate();
      }
    });
    await listen("tally-updated", (event) => {
      window.StatTracker.stats = event.payload.stats;
      window.StatTracker.online = event.payload.online;
      if (window.StatTracker.config) {
        sendUpdate();
      }
    });
    await listen("update-available", async (event) => {
      const payload = event.payload || {};
      const version = payload.version || payload;
      const current = String(payload.currentVersion || "").replace(/^v/i, "");
      const notes = String(payload.notes || "").trim();
      let message = `StatTracker ${version} is available`;
      if (current && current !== String(version).replace(/^v/i, "")) {
        message += ` (you have ${current})`;
      }
      message += ". Install it now?";
      if (notes) {
        message += `\n\n${notes}`;
      }
      const accepted = await dialog.ask(message, {
        title: "Update available",
        type: "info",
      });
      if (accepted) {
        await invoke("install_update");
      }
    });

    const bootstrap = await invoke("get_bootstrap");
    if (bootstrap.configured) {
      await render(true);
    } else {
      await invoke("open_settings");
    }
  }

  window.addEventListener("DOMContentLoaded", () => {
    boot().catch((error) => {
      dialog.message(String(error), { title: "StatTracker", type: "error" });
    });
  });
})();
