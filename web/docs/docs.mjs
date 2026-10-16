// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * RangerDocx in a page: one call puts the Word editor into an element.
 *
 *   import { mountDocs } from "./docs.mjs";
 *   const doc = await mountDocs(document.getElementById("here"), {
 *     ui: "full",              // "full" | "compact" | "viewer"
 *     theme: "light",          // "light" | "dark"
 *     readOnly: false,
 *     docx: "report.docx",     // a URL or an ArrayBuffer
 *     name: "report.docx",
 *     base: "./",              // where gl/, fonts/, docx_shell.js and docs.css are
 *   });
 *   const bytes = doc.saveBytes();  // the document as .docx
 *
 * Everything is drawn by EVG into one <canvas> through WebGL 2: the EVGUI
 * chrome (header, tabs, ribbon, status line, dialogs) and the Word core inside
 * it. A screen reader gets the app's accessibility tree mirrored as DOM over
 * the canvas (gl/evg-a11y.js). The same host as EVGSheets' evgsheets.mjs.
 */

const FONTS = [
  ["Open Sans", "OpenSans-Regular.ttf", { family: "Open Sans", weight: "400", style: "normal" }],
  [null, "OpenSans-Bold.ttf", { family: "Open Sans", weight: "700", style: "normal" }],
  [null, "OpenSans-Italic.ttf", { family: "Open Sans", weight: "400", style: "italic" }],
  [null, "OpenSans-BoldItalic.ttf", { family: "Open Sans", weight: "700", style: "italic" }],
  // The fallback pool: emoji, the geometric bullets, Arabic. The core falls
  // back per codepoint across every loaded face; the browser has to know the
  // same faces or it measures one font and draws another.
  [null, "NotoEmoji-Regular.ttf", { family: "Noto Emoji", weight: "400", style: "normal" }],
  [null, "NotoSans-Regular.ttf", { family: "Noto Sans", weight: "400", style: "normal" }],
  [null, "ElMessiri-Regular.ttf", { family: "El Messiri", weight: "400", style: "normal" }],
  [null, "ElMessiri-Bold.ttf", { family: "El Messiri", weight: "700", style: "normal" }],
];

const KEY_NAMES = new Set([
  "Backspace", "Enter", "Tab", "Escape", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown",
  "Delete", "Home", "End", "PageUp", "PageDown", "F6", "F10",
]);
// Ctrl/Cmd chords the editor answers. Also what keeps Ctrl+S from saving the
// web page and Ctrl+F from opening the browser's find bar.
const CTRL_CHORD = /^[abdefhilopsuyzABDEFHILOPSUYZ=+]$/;

const shared = new Map();

function asRangerBuffer(ab) {
  ab._view = new DataView(ab);
  return ab;
}

async function fetchBytes(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(url + " → " + res.status);
  return asRangerBuffer(await res.arrayBuffer());
}

function loadScript(src) {
  return new Promise((resolve, reject) => {
    const s = document.createElement("script");
    s.src = src;
    s.onload = resolve;
    s.onerror = () => reject(new Error("could not load " + src));
    document.head.appendChild(s);
  });
}

/** The engine, the renderer, the stylesheet and the fonts — once per base. */
function loadShared(base) {
  if (shared.has(base)) return shared.get(base);
  const p = (async () => {
    const stamp = globalThis.RANGERDOCX_BUILD ? "?v=" + globalThis.RANGERDOCX_BUILD : "";
    if (!globalThis.RangerDocxModule) await loadScript(base + "docx_shell.js" + stamp);
    const mod = globalThis.RangerDocxModule;
    const gl = await import(new URL(base + "gl/evg-webgl.js" + stamp, location.href).href);
    const a11y = await import(new URL(base + "gl/evg-a11y.js" + stamp, location.href).href);
    const measure = await import(new URL(base + "gl/evg-measure.js" + stamp, location.href).href);
    const css = await (await fetch(base + "docs.css" + stamp)).text();
    const faces = await Promise.all(FONTS.map(([, file]) => fetchBytes(base + "fonts/" + file)));
    if (typeof FontFace === "function" && document.fonts) {
      await Promise.all(FONTS.map(async ([, file, cssFace], i) => {
        try {
          const face = new FontFace(cssFace.family, faces[i].slice(0), { weight: cssFace.weight, style: cssFace.style });
          await face.load();
          document.fonts.add(face);
        } catch (e) {
          console.warn("rangerdocx: could not register " + file, e);
        }
      }));
    }
    gl.setFontFallback([...new Set(FONTS.map(([, , c]) => c.family))]);
    await document.fonts.ready;
    // The chrome's layout measures with the browser's own text metrics.
    const measurer = measure.installCanvasMeasurer([mod]);
    measurer.refresh();
    return { mod, gl, a11y, css, faces };
  })();
  shared.set(base, p);
  return p;
}

function boolOpt(v) {
  if (v === undefined || v === null) return undefined;
  if (typeof v === "boolean") return v;
  return v === "" || /^(1|true|yes|on)$/i.test(String(v));
}

/** Options from a query string: ?ui=viewer&theme=dark&docx=<url>… */
export function optionsFromQuery(search = location.search) {
  const q = new URLSearchParams(search);
  const o = {};
  for (const k of ["ui", "theme", "docx", "name", "tab"]) if (q.has(k)) o[k] = q.get(k);
  for (const k of ["readOnly", "header", "ribbon", "status", "nav", "a11y"]) {
    const v = q.get(k) ?? q.get(k.toLowerCase());
    if (v !== null) o[k] = boolOpt(v);
  }
  if (q.has("docx") && !q.has("name")) o.name = q.get("docx").split("/").pop();
  return o;
}

export async function mountDocs(container, options = {}) {
  const base = options.base || "./";
  const { mod, gl: glMod, a11y: a11yMod, css, faces } = await loadShared(base);

  container.classList.add("rangerdocx-host");
  const wrap = document.createElement("div");
  wrap.className = "rangerdocx-wrap";
  wrap.style.cssText = "position:relative;width:100%;height:100%;overflow:hidden;outline:none";
  const canvas = document.createElement("canvas");
  canvas.tabIndex = 0;
  canvas.setAttribute("aria-label", options.label || "Document editor");
  canvas.style.cssText = "display:block;width:100%;height:100%;outline:none;touch-action:none";
  wrap.appendChild(canvas);
  container.appendChild(wrap);

  const gl = canvas.getContext("webgl2", { antialias: true, premultipliedAlpha: false, stencil: true });
  if (!gl) throw new Error("RangerDocx needs WebGL 2");

  const sizeOf = () => {
    const r = wrap.getBoundingClientRect();
    return { w: Math.max(320, Math.floor(r.width)), h: Math.max(240, Math.floor(r.height)) };
  };
  let { w, h } = sizeOf();
  const app = new mod.DocsApp();
  app.start(w, h, css);
  FONTS.forEach(([family], i) => {
    if (family) app.addFont(family, faces[i]);
    else app.addFace(faces[i]);
  });
  if (options.ui) app.setPreset(options.ui);
  const opt = (name, v) => {
    if (v === undefined || v === null) return;
    app.setOption(name, String(v));
  };
  opt("header", options.header);
  opt("ribbon", options.ribbon);
  opt("status", options.status);
  opt("nav", options.nav);
  opt("readOnly", options.readOnly);
  if (options.title) opt("name", options.title);
  if (options.theme) app.setTheme(options.theme);
  else if (options.theme !== false && window.matchMedia?.("(prefers-color-scheme: dark)").matches) app.setTheme("dark");

  // --- the document -----------------------------------------------------------
  let docName = options.name || "document.docx";
  let note = "";
  async function openBytes(raw, name) {
    const ok = app.openDocument(asRangerBuffer(raw), name || "document.docx");
    note = ok ? "" : app.web.note;
    if (ok) docName = name || docName;
    refreshMedia();
    await draw(true);
    return ok;
  }
  if (options.docx instanceof ArrayBuffer) {
    if (!app.openDocument(asRangerBuffer(options.docx.slice(0)), docName)) note = app.web.note;
  } else if (typeof options.docx === "string" && options.docx) {
    try {
      const raw = await (await fetch(options.docx)).arrayBuffer();
      if (!app.openDocument(asRangerBuffer(raw), docName)) note = app.web.note;
    } catch (e) {
      console.warn("rangerdocx: could not open " + options.docx, e);
    }
  }
  if (options.tab) {
    app.showTab(options.tab);
    app.sync();
  }

  // --- pictures in the document -----------------------------------------------
  const imageCache = new Map();
  const blobUrls = new Map();
  function refreshMedia() {
    for (const url of blobUrls.values()) if (url) URL.revokeObjectURL(url);
    blobUrls.clear();
    imageCache.clear();
    let parts = [];
    try {
      parts = JSON.parse(app.web.imageParts() || "[]");
    } catch (_) {
      parts = [];
    }
    for (const part of parts) {
      const raw = app.web.imageBytes(part);
      const view = raw instanceof ArrayBuffer ? new Uint8Array(raw) : raw;
      if (!view || !(view.length || view.byteLength)) continue;
      const type = /\.png$/i.test(part) ? "image/png" : "image/jpeg";
      blobUrls.set(part, URL.createObjectURL(new Blob([view], { type })));
    }
  }
  async function imagesFor(doc) {
    const wanted = new Set(doc.list.cmds.filter((c) => c.k === 2 && c.src).map((c) => c.src));
    for (const src of wanted) {
      if (imageCache.has(src)) continue;
      const url = blobUrls.get(src) || "";
      if (!url) { imageCache.set(src, null); continue; }
      const got = await glMod.loadImages({ list: { cmds: [{ k: 2, src: url }] } }, { base: "" });
      imageCache.set(src, got.get(url) || null);
    }
    const out = new Map();
    for (const src of wanted) out.set(src, imageCache.get(src) || null);
    return out;
  }
  refreshMedia();

  // --- the accessibility mirror -------------------------------------------------
  const mirror = options.a11y === false ? null : a11yMod.createA11yMirror(wrap, {
    canvas,
    label: options.label || "Document editor",
    onActivate: async (node) => {
      a11yMod.pressAtCentre(node, (x, y) => {
        app.pointer(x, y, true, false, false);
        app.pointer(x, y, false, false, false);
      });
      await after();
    },
  });

  // --- drawing ------------------------------------------------------------------
  let lastScene = "";
  let destroyed = false;
  let frames = 0;
  async function draw(force) {
    if (destroyed) return;
    const text = app.scene();
    if (!force && text === lastScene) return;
    lastScene = text;
    const doc = JSON.parse(text);
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const bw = Math.round(doc.width * dpr);
    const bh = Math.round(doc.height * dpr);
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
    }
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(1, 1, 1, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
    controller.stats = glMod.renderDisplayList(gl, doc, { dpr, images: await imagesFor(doc) });
    frames += 1;
    if (mirror) {
      try {
        mirror.update(JSON.parse(app.a11y()));
      } catch (e) {
        console.warn("rangerdocx a11y:", e);
      }
    }
    controller.lastDoc = doc;
    if (app.tick(0)) animate();
  }

  // --- files, printing, the clipboard ---------------------------------------------
  const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document";
  fileInput.style.display = "none";
  wrap.appendChild(fileInput);
  fileInput.addEventListener("change", async () => {
    const f = fileInput.files && fileInput.files[0];
    if (!f) return;
    await openBytes(await f.arrayBuffer(), f.name);
    fileInput.value = "";
  });

  function saveBlob(blob, name) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.style.display = "none";
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  async function serveFileRequest() {
    const want = app.takeFileRequest();
    if (!want) return;
    if (want === "open") {
      if (options.onOpenRequest) options.onOpenRequest(controller);
      else fileInput.click();
    } else if (want === "saveAs") {
      const raw = app.web.saveDocument();
      if (options.onSave) options.onSave(raw, docName, controller);
      else saveBlob(new Blob([raw], { type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" }), docName);
    } else if (want === "print") {
      window.print();
    } else if (want === "copy") {
      const text = app.web.takeClipboard();
      if (text) navigator.clipboard?.writeText(text).catch(() => {});
    } else if (want === "paste") {
      try {
        const text = await navigator.clipboard.readText();
        if (text) app.paste("", text);
      } catch (_) {
        /* the browser did not allow reading the clipboard: Ctrl+V still works */
      }
    }
  }

  let changeTimer = 0;
  async function after() {
    await serveFileRequest();
    await draw();
    if (options.onChange) {
      clearTimeout(changeTimer);
      changeTimer = setTimeout(() => options.onChange(controller), 120);
    }
    animate();
  }

  // Submenus open on a delay and transitions run on a clock.
  let ticking = false;
  function animate() {
    if (ticking) return;
    ticking = true;
    let t0 = performance.now();
    const step = async (now) => {
      const busy = app.tick(now - t0);
      t0 = now;
      await draw();
      if (busy && !destroyed) requestAnimationFrame(step);
      else ticking = false;
    };
    requestAnimationFrame(step);
  }

  // --- input ----------------------------------------------------------------------
  const at = (ev) => {
    const r = canvas.getBoundingClientRect();
    return { x: Math.floor(ev.clientX - r.left), y: Math.floor(ev.clientY - r.top) };
  };
  let down = false;
  let lastCursor = "";
  canvas.addEventListener("mousedown", (ev) => ev.preventDefault());
  canvas.addEventListener("pointerdown", async (ev) => {
    if (ev.button === 2) return;
    canvas.setPointerCapture(ev.pointerId);
    if (!mirror) canvas.focus();
    else if (!wrap.contains(document.activeElement)) canvas.focus();
    down = true;
    const { x, y } = at(ev);
    app.pointer(x, y, true, ev.shiftKey, ev.ctrlKey || ev.metaKey);
    await after();
  });
  canvas.addEventListener("pointermove", async (ev) => {
    const { x, y } = at(ev);
    const want = app.cursorAt(x, y);
    if (want !== lastCursor) {
      canvas.style.cursor = want;
      lastCursor = want;
    }
    app.pointer(x, y, down, ev.shiftKey, ev.ctrlKey || ev.metaKey);
    await after();
  });
  const release = async (ev) => {
    if (!down) return;
    down = false;
    const { x, y } = at(ev);
    app.pointer(x, y, false, ev.shiftKey, ev.ctrlKey || ev.metaKey);
    await after();
  };
  canvas.addEventListener("pointerup", release);
  canvas.addEventListener("pointercancel", release);
  canvas.addEventListener("dblclick", async (ev) => {
    const { x, y } = at(ev);
    app.doubleClick(x, y);
    await after();
  });
  canvas.addEventListener("wheel", async (ev) => {
    ev.preventDefault();
    const { x, y } = at(ev);
    if (ev.ctrlKey) {
      app.zoomBy(ev.deltaY < 0 ? 1.1 : 1 / 1.1);
      await after();
      return;
    }
    let dy = ev.deltaY;
    if (ev.deltaMode === 1) dy *= 16;
    else if (ev.deltaMode === 2) dy *= 400;
    if (app.wheel(x, y, Math.round(dy))) await after();
  }, { passive: false });

  // Keys are taken on the wrapper: with the mirror up, focus is on one of its
  // DOM nodes and the canvas never sees the event.
  wrap.addEventListener("keydown", async (ev) => {
    if (options.onKey && options.onKey(ev, controller) === false) return;
    const ctrl = ev.ctrlKey || ev.metaKey;
    if (KEY_NAMES.has(ev.key)) {
      ev.preventDefault();
      app.key(ev.key, ev.shiftKey, ctrl);
      await after();
      return;
    }
    if (ctrl) {
      const k = ev.key.toLowerCase();
      // Copy and cut write the system clipboard here; paste comes through the
      // paste event, which carries the html flavour (tables, charts).
      if (k === "c" || k === "x") {
        const text = k === "c" ? app.copySelection() : app.cutSelection();
        if (text) {
          ev.preventDefault();
          navigator.clipboard?.writeText(text).catch(() => {});
          await after();
        }
        return;
      }
      if (k === "v") return;
      if (CTRL_CHORD.test(ev.key)) {
        ev.preventDefault();
        app.text(ev.key, ev.shiftKey, true);
        await after();
      }
      return;
    }
    if (ev.key.length === 1) {
      ev.preventDefault();
      app.text(ev.key, ev.shiftKey, false);
      await after();
    }
  });
  const onPaste = async (ev) => {
    if (!wrap.contains(document.activeElement)) return;
    ev.preventDefault();
    const cd = ev.clipboardData;
    app.paste(cd ? cd.getData("text/html") : "", cd ? cd.getData("text/plain") : "");
    await after();
  };
  window.addEventListener("paste", onPaste);

  const ro = new ResizeObserver(async () => {
    const s = sizeOf();
    if (s.w === w && s.h === h) return;
    w = s.w;
    h = s.h;
    app.resize(w, h);
    await draw(true);
  });
  ro.observe(wrap);
  // The caret blinks on the core's clock.
  const blink = setInterval(() => draw(), 600);

  const controller = {
    app,
    canvas,
    element: wrap,
    lastDoc: null,
    stats: null,
    get note() { return note; },
    open: openBytes,
    async openUrl(url, name) {
      const raw = await (await fetch(url)).arrayBuffer();
      return openBytes(raw, name || url.split("/").pop());
    },
    saveBytes: () => app.web.saveDocument(),
    state: () => JSON.parse(app.state()),
    async setPreset(name) { app.setPreset(name); await draw(true); },
    async setOption(name, value) { app.setOption(name, String(value)); await draw(true); },
    async setTheme(name) { app.setTheme(name); await draw(true); },
    async run(id, arg = "") { const ok = app.web.run(id, arg); app.sync(); await after(); return ok; },
    async press(tid) { const ok = app.pressId(tid); await after(); return ok; },
    async key(name, shift = false, ctrl = false) { app.key(name, shift, ctrl); await after(); },
    async type(text) { for (const ch of text) app.text(ch, false, false); await after(); },
    focus() { canvas.focus(); },
    redraw: () => draw(true),
    frames: () => frames,
    destroy() {
      destroyed = true;
      clearInterval(blink);
      ro.disconnect();
      window.removeEventListener("paste", onPaste);
      for (const url of blobUrls.values()) if (url) URL.revokeObjectURL(url);
      wrap.remove();
    },
  };
  await draw(true);
  return controller;
}
