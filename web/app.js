import { createKioskController } from "/assets/kiosk.js";
import { kioskView } from "/assets/kiosk-view.js";
const app = document.querySelector("#app");
let data, selected, toastTimer, kiosk;
let hostEpoch = 0, hostRequest, hostDraft;
const sessionChannel = typeof BroadcastChannel === "function" ? new BroadcastChannel("houseworks-session") : null;
function announceSignout() {
  sessionChannel?.postMessage("signed-out");
  try { localStorage.setItem("houseworks-signout", crypto.randomUUID()); } catch {}
}
const kioskRoute = location.pathname.match(/^\/kiosk\/([0-9a-f-]{36})$/i);
const $ = (selector, root = document) => root.querySelector(selector);
const esc = (value) =>
  String(value ?? "").replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ],
  );
const icons = {
  expand: '<path d="M8 3H3v5M16 3h5v5M3 16v5h5M21 16v5h-5"/>',
  home: '<path d="m3 10 9-7 9 7M5 9v12h5v-7h4v7h5V9"/>',
  people:
    '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M22 21v-2a4 4 0 0 0-3-3.87M15 3a4 4 0 0 1 0 8"/><circle cx="9" cy="7" r="4"/>',
  settings:
    '<path d="m9 3-1 3-3 1-2 3 2 3v4l4 1 3 3 3-3 4-1v-4l2-3-2-3-3-1-1-3z"/><circle cx="12" cy="12" r="3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  external: '<path d="M15 3h6v6M21 3l-9 9M10 3H3v18h18v-7"/>',
  qr: '<path d="M3 3h6v6H3zM15 3h6v6h-6zM3 15h6v6H3zM15 15h3v3h3v3h-6zM12 3v3M3 12h3M12 12h3M21 12v3M12 18v3"/>',
  download: '<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>',
  refresh:
    '<path d="M20 7v5h-5M4 17v-5h5M5 7a8 8 0 0 1 13-2l2 3M4 16l2 3a8 8 0 0 0 13-2"/>',
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>',
  arrow: '<path d="M4 12h16m-6-6 6 6-6 6"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
};
function icon(name) {
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name] || ""}</svg>`;
}
function brand() {
  return '<a class="brand" href="/" aria-label="Open Houseworks home"><span class="door" aria-hidden="true"></span><span class="wordmark"><span>Open</span><span>Houseworks</span></span></a>';
}
function toast(message) {
  const el = $("#toast");
  el.textContent = message;
  el.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("show"), 4000);
}
async function api(path, options = {}) {
  let res;
  try {
    res = await fetch(path, {
      credentials: "same-origin",
      ...options,
      headers: {
        "Content-Type": "application/json",
        "x-houseworks-request": "1",
        ...options.headers,
      },
    });
  } catch (error) {
    if (error.name === "TimeoutError") throw error;
    throw new Error(
      "Couldn’t reach the server. Check the connection and try again.",
    );
  }
  if (res.status === 204) return null;
  const body = await res
    .json()
    .catch(() => ({ error: "The server could not complete this request." }));
  if (!res.ok) {
    const error = new Error(
      body.error || "Something went wrong. Please try again.",
    );
    error.status = res.status;
    throw error;
  }
  return body;
}
function errorMessage(error, form) {
  const node = $(".form-error", form);
  if (node) {
    node.textContent = error.message;
    node.setAttribute("role", "alert");
  } else toast(error.message);
}
function dateLabel(h) {
  try {
    const date = new Intl.DateTimeFormat(undefined, {
      weekday: "long",
      month: "long",
      day: "numeric",
      timeZone: h.timezone,
    }).format(new Date(h.starts_at));
    const time = new Intl.DateTimeFormat(undefined, {
      hour: "numeric",
      minute: "2-digit",
      timeZone: h.timezone,
    });
    return `${date} · ${time.format(new Date(h.starts_at))}–${time.format(new Date(h.ends_at))}`;
  } catch {
    return "Time to be confirmed";
  }
}
function brandColor(color, el = document.documentElement) {
  if (!/^#[0-9a-f]{6}$/i.test(color)) return;
  const channels = [1, 3, 5]
    .map((i) => parseInt(color.slice(i, i + 2), 16) / 255)
    .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  const luminance =
    0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
  el.style.setProperty("--brand", color);
  el.style.setProperty("--brand-ink", luminance > 0.183 ? "#152d25" : color);
  el.style.setProperty("--on-brand", luminance > 0.179 ? "#101b15" : "#ffffff");
}
function input(name, label, value = "", options = {}) {
  const id = `${options.prefix || ""}${name}`;
  return `<label for="${id}">${label}<input id="${id}" name="${name}" type="${options.type || "text"}" value="${esc(value)}" ${options.required ? "required" : ""} ${options.max ? `maxlength="${options.max}"` : ""} ${options.autocomplete ? `autocomplete="${options.autocomplete}"` : ""} ${options.placeholder ? `placeholder="${esc(options.placeholder)}"` : ""}></label>`;
}
function optional(label) {
  return `${label} <span class="optional">(optional)</span>`;
}
function contactIdentity(name, imageUrl, kind) {
  if (!name && !imageUrl) return "";
  const fallback = kind === "agent-photo" ? "Agent photo" : "Brokerage logo";
  return `<div class="contact-identity">${imageUrl ? `<img class="contact-image ${kind}" src="${esc(imageUrl)}" alt="${name ? "" : fallback}" width="48" height="48" referrerpolicy="no-referrer">` : ""}${name ? `<h3>${esc(name)}</h3>` : ""}</div>`;
}
function contact(s) {
  const agent = !!(
    s.host_name ||
    s.agent_photo_url ||
    s.agent_license ||
    s.agent_phone ||
    s.contact_email
  );
  const broker = !!(
    s.business_name ||
    s.logo_url ||
    s.broker_name ||
    s.broker_license ||
    s.broker_phone ||
    s.broker_email
  );
  if (!agent && !broker) return "";
  const links = (email, phone) =>
    `<div class="contact-links">${email ? `<a href="mailto:${esc(email)}">${esc(email)}</a>` : ""}${phone ? `<a href="tel:${esc(phone.replace(/[^+\d]/g, ""))}">${esc(phone)}</a>` : ""}</div>`;
  return `<footer class="host-contact">${agent ? `<div class="agent"><span class="eyebrow">Your host</span>${contactIdentity(s.host_name, s.agent_photo_url, "agent-photo")}${s.agent_license ? `<p>Agent license: ${esc(s.agent_license)}</p>` : ""}${links(s.contact_email, s.agent_phone)}</div>` : ""}${broker ? `<div class="${agent ? "broker" : ""}">${contactIdentity(s.business_name, s.logo_url, "brokerage-logo")}${s.broker_name ? `<p>Broker: ${esc(s.broker_name)}</p>` : ""}${s.broker_license ? `<p>Broker license: ${esc(s.broker_license)}</p>` : ""}${links(s.broker_email, s.broker_phone)}</div>` : ""}</footer>`;
}
function guestForm(s, preview = false, shared = false) {
  const prefix = preview ? "preview-" : "";
  return `<form class="guest-form" autocomplete="${shared ? "off" : "on"}" ${preview ? 'inert aria-hidden="true"' : 'id="guest-form"'}>
 ${input("name", "Full name", "", { prefix, required: true, max: 120, autocomplete: shared ? "off" : "name", placeholder: "e.g. Taylor Kim" })}
 ${input("email", "Email address", "", { prefix, type: "email", required: true, max: 254, autocomplete: shared ? "off" : "email", placeholder: "you@example.com" })}
 ${s.ask_phone ? input("phone", optional("Phone number"), "", { prefix, type: "tel", max: 40, autocomplete: shared ? "off" : "tel", placeholder: "Your phone number" }) : ""}
 ${s.ask_timeline ? `<label for="${prefix}timeline">${optional(shared ? "Buying timeline" : "When are you hoping to buy?")}<select id="${prefix}timeline" name="timeline"><option value="">Select a timeline</option>${["Exploring", "0–3 months", "3–6 months", "6+ months"].map((v) => `<option>${v}</option>`).join("")}</select></label>` : ""}
 ${s.ask_agent ? `<fieldset><legend>${optional(shared ? "Working with an agent?" : "Are you working with an agent?")}</legend><div class="radios">${["Yes", "No", "Prefer not to say"].map((v, i) => `<label class="choice" for="${prefix}agent-${i}"><input id="${prefix}agent-${i}" type="radio" name="represented" value="${v}">${v}</label>`).join("")}</div></fieldset>` : ""}
 <label class="choice consent" for="${prefix}follow-up"><input type="checkbox" name="follow_up" id="${prefix}follow-up">${esc(data?.consent_text || "I'd like the host to follow up with me about this property.")}</label>
 <p class="privacy">${esc(s.privacy_note)}</p>
 <label class="honeypot" aria-hidden="true">Leave this empty<input type="text" name="website" tabindex="-1" autocomplete="off"></label>
 <p class="form-error" aria-live="polite"></p>
 <button class="btn primary full guest-submit" type="submit">Check in</button><p class="no-account">No account needed.</p></form>`;
}
function propertyTitle(h, s) {
  return `<div class="property-title"><h1>${esc(h.address)}</h1>${s.show_location && h.location ? `<p class="property-location">${esc(h.location)}</p>` : ""}</div>`;
}
function guestHeader(h, s) {
  return `${h.photo_url ? `<img class="guest-photo" src="${esc(h.photo_url)}" alt="${esc(h.address)}">` : ""}<div class="guest-main"><p class="guest-kicker">Welcome to</p>${propertyTitle(h, s)}<p class="date">${esc(dateLabel(h))}</p>${s.welcome ? `<p class="guest-welcome">${esc(s.welcome)}</p>` : "<hr>"}`;
}
async function loadGuest(id, print = false, shared = false) {
  try {
    data = await api(`/api/public/houses/${id}`);
    const { house: h, settings: s } = data;
    brandColor(s.color);
    document.title = `${print ? "Open house" : "Welcome to"} ${h.address} · ${s.business_name || "Open Houseworks"}`;
    if (shared) {
      app.innerHTML = kioskView(h, s, {
        esc,
        icon,
        form: guestForm(s, false, true),
        contact: contact(s),
        dateLabel,
        propertyTitle,
      });
      kiosk.rendered();
      return;
    }
    if (print) {
      app.innerHTML = `<div class="print-controls row"><a class="btn" href="/">Back to workspace</a><button class="btn primary" data-action="print">Print sign</button></div><main id="main" class="print-sheet"><p class="eyebrow">${esc(s.business_name || "You’re invited")}</p><h1>Come on in.</h1><p>${esc(h.address)}${s.show_location && h.location ? ` · <span class="property-location">${esc(h.location)}</span>` : ""}</p><p class="muted">${esc(dateLabel(h))}</p><img class="qr" src="/api/public/houses/${h.id}/qr.svg" alt="Scan to check in at this open house"><h2>Scan to check in</h2><p class="sub">A quick hello. No account needed.</p><p class="url">${esc(location.origin)}/visit/${h.id}</p>${contact(s)}</main>`;
      return;
    }
    app.innerHTML = `<main id="main" class="guest-shell">${guestHeader(h, s)}${h.status === "open" ? guestForm(s) : '<section class="thank-you"><h2>Thanks for your interest.</h2><p>Check-in for this open house has closed. You can reach the host below.</p></section>'}${h.note ? `<p class="small-note">${esc(h.note)}</p>` : ""}${contact(s)}<p class="guest-foot">Open Houseworks</p></div></main>`;
  } catch (error) {
    app.innerHTML = `<main id="main" class="guest-shell"><div class="guest-main thank-you"><span class="door" aria-hidden="true"></span><h1>This door isn’t open.</h1><p class="sub">${esc(error.status === 404 ? "The check-in link is unavailable. Please ask your host for the current QR code." : error.message)}</p><button class="btn" data-action="reload">Try again</button></div></main>`;
  }
}
function loginPage(message = "") {
  document.title = "Host sign-in · Open Houseworks";
  app.innerHTML = `<main id="main" class="login"><div class="login-image" role="img" aria-label="An illustrative California craftsman home"></div><div class="login-content">${brand()}<h1>A better welcome<br>starts here.</h1><p>Your open houses, visitors, and next conversations.<br>A little more organized.</p><form id="login-form" class="stack">${input("password", "Host password", "", { type: "password", required: true, max: 256, autocomplete: "current-password" })}<p class="form-error" aria-live="polite">${esc(message)}</p><button class="btn primary full" type="submit">Open your workspace ${icon("arrow")}</button></form><p class="login-footer">Here to visit an open house? Use your host’s QR code.<br>Sample property image shown.</p></div></main>`;
}
function shell(content, page) {
  return `<div class="workspace"><aside class="sidebar">${brand()}<nav class="nav" aria-label="Workspace">${[
    ["/", "home", "Open houses"],
    ["/visitors", "people", "Visitors"],
    ["/customize", "settings", "Customize"],
  ]
    .map(
      ([href, i, label]) =>
        `<a href="${href}" ${page === href ? 'aria-current="page"' : ""}>${icon(i)}${label}</a>`,
    )
    .join(
      "",
    )}</nav><div class="sidebar-bottom"><p>Free tools for<br>a more open market.</p><a href="https://github.com/BenBreaksIn/open-house-register" target="_blank" rel="noopener">Project on GitHub</a></div></aside><main id="main" class="main"><div class="topline"><span class="eyebrow">Your open-house workspace</span><div class="row"><button class="icon-button" data-action="refresh" aria-label="Check for updates" title="Check for updates">${icon("refresh")}</button><button class="btn quiet" data-action="logout">Sign out</button></div></div>${content}</main></div>`;
}
function visitorsTable(visitors, all = false) {
  if (!visitors.length)
    return '<div class="table-empty">No visitors yet. Share your check-in page or print a QR sign to get started.</div>';
  return `<div class="table-scroll"><table class="visitor-table"><thead><tr><th scope="col">Name</th><th scope="col">Contact</th><th scope="col">${all ? "Open house" : "Buying timeline"}</th><th scope="col">Follow-up</th><th scope="col"><span class="hidden">Actions</span></th></tr></thead><tbody>${visitors.map((v) => `<tr data-visitor="${v.id}" data-search="${esc(`${v.name} ${v.email} ${v.phone}`.toLowerCase())}"><td>${esc(v.name)}<span class="muted">${esc(new Date(v.checked_in_at).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }))}</span></td><td><a href="mailto:${esc(v.email)}">${esc(v.email)}</a>${v.phone ? `<span class="muted">${esc(v.phone)}</span>` : ""}</td><td>${esc(all ? data.houses.find((h) => h.id === v.house_id)?.address || "Open house" : v.timeline || "Not shared")}</td><td>${v.follow_up ? "Requested" : "Not requested"}</td><td><button class="icon-button danger" data-action="delete-visitor" data-id="${v.id}" aria-label="Delete ${esc(v.name)}" title="Delete visitor">${icon("trash")}</button></td></tr>`).join("")}</tbody></table></div><p id="no-matches" class="notice hidden">No matching visitors.</p>`;
}
function dashboard() {
  const h = data.houses.find((h) => h.id === selected) || data.houses[0];
  selected = h?.id;
  const heading = `<header class="heading"><div><h1>A warm welcome.<br>A clear next step.</h1><p class="sub">Make room for the people, not the paperwork.</p></div><button class="btn primary" data-action="new-house">${icon("plus")} Create open house</button></header>`;
  if (!h)
    return shell(
      `${heading}<section class="empty"><span class="door" aria-hidden="true"></span><h2>Let’s open your first door.</h2><p>Create an open house, share its check-in page, and welcome visitors. Their details stay in your workspace.</p><div class="row empty-actions"><button class="btn primary" data-action="new-house">Create open house ${icon("arrow")}</button><a class="btn quiet" href="/customize">Add your branding</a></div></section>`,
      "/",
    );
  const visitors = data.visitors.filter((v) => v.house_id === h.id);
  const capacity = data.capacity;
  const capacityNote = capacity ? `<p class="small-note" role="status">${Number(capacity.events[h.id] || 0).toLocaleString()} of ${Number(capacity.event_limit).toLocaleString()} registrations for this open house · ${Number(capacity.total).toLocaleString()} of ${Number(capacity.total_limit).toLocaleString()} across your workspace.${Number(capacity.events[h.id] || 0) >= capacity.event_limit || capacity.total >= capacity.total_limit ? " Check-in is full. Export and remove records you no longer need, or ask your administrator to increase capacity." : ""}</p>` : "";
  return shell(
    `${heading}${capacityNote}<div class="rule"><div class="row"><div class="house-picker"><label class="hidden" for="house-selector">Choose an open house</label><select id="house-selector">${data.houses.map((x) => `<option value="${x.id}" ${x.id === h.id ? "selected" : ""}>${esc(x.address)} · ${esc(x.status)}</option>`).join("")}</select></div><span class="spacer"></span><button class="btn quiet" data-action="edit-house" data-id="${h.id}">Edit details</button></div><section class="property"><div>${h.photo_url ? `<img class="property-image" src="${esc(h.photo_url)}" alt="${esc(h.address)}">` : `<div class="photo-empty">${icon("home")}</div>`}${h.photo_url === "/assets/sample-house.png" ? '<p class="small-note">Illustrative sample photo</p>' : ""}</div><div><h2>${esc(h.address)}</h2><p class="place">${esc(h.location)}</p><p class="date">${esc(dateLabel(h))}</p><p class="small-note">${h.status === "open" ? "Accepting check-ins" : h.status === "draft" ? "Draft · only visible to you" : "Check-in closed"}</p>${h.status === "draft" ? `<div class="row property-actions"><button class="btn primary" data-action="publish-house" data-id="${h.id}">Open check-in page ${icon("arrow")}</button></div>` : `<div class="checkin-options"><section class="checkin-option"><h3>Use this device</h3><a class="btn primary" href="/kiosk/${h.id}">${icon("expand")} Launch kiosk</a><p>Let visitors type on your tablet or computer. Signs this device out of the host workspace.</p></section><section class="checkin-option"><h3>Use their own phone</h3><a class="btn" target="_blank" rel="noopener" href="/sign/${h.id}">${icon("qr")} Share QR code</a><p>Display or print the code. Visitors check in on their own device.</p></section></div><a class="phone-preview-link" target="_blank" rel="noopener" href="/visit/${h.id}">Preview phone check-in</a>`}<div class="stats"><div class="stat"><strong>${visitors.length}</strong><span>Visitors</span></div><div class="stat"><strong>${visitors.filter((v) => v.follow_up).length}</strong><span>Requested follow-up</span></div><div class="stat"><strong>${visitors.filter((v) => v.timeline === "0–3 months").length}</strong><span>Buying in 0–3 months</span></div></div></div></section><section class="rule"><div class="section-head"><h2>Your visitors</h2><div class="table-tools"><label class="hidden" for="visitor-search">Search visitors</label><input id="visitor-search" type="search" placeholder="Search by name or email…"><a class="btn" href="/api/admin/houses/${h.id}/export">${icon("download")} Export CSV</a></div></div>${visitorsTable(visitors)}</section></div>`,
    "/",
  );
}
function allVisitors() {
  return shell(
    `<header class="heading"><div><h1>Good to meet you.</h1><p class="sub">Every visitor, across every open house.</p></div></header><section class="rule"><div class="section-head"><h2>${data.visitors.length} visitors</h2><div class="table-tools"><label class="hidden" for="visitor-search">Search visitors</label><input id="visitor-search" type="search" placeholder="Search by name or email…"></div></div>${visitorsTable(data.visitors, true)}</section>`,
    "/visitors",
  );
}
function preview(s) {
  const h = data.houses.find((h) => h.id === selected) ||
    data.houses[0] || {
      address: "248 Olive Avenue",
      location: "Oakland, CA",
      starts_at: "2026-10-17T20:00:00Z",
      ends_at: "2026-10-17T23:00:00Z",
      timezone: "America/Los_Angeles",
      photo_url: "/assets/sample-house.png",
    };
  return `${guestHeader(h, s)}${guestForm(s, true)}${contact(s)}</div>`;
}
function customize() {
  const s = data.settings;
  const field = (name, label, type = "text", max = 80) =>
    input(name, optional(label), s[name], { type, max });
  const toggle = (name, label) =>
    `<label class="choice"><input type="checkbox" role="switch" name="${name}" ${s[name] ? "checked" : ""}>${label}</label>`;
  return shell(
    `<header class="heading"><div><h1>Make yourself at home.</h1><p class="sub">Your name. Your colors. Your welcome.</p></div></header><div class="customize-layout"><form id="settings-form"><section class="form-section"><h2>Your brand</h2><p class="muted">Every field is optional. Empty details stay hidden from visitors.</p><div class="pair">${field("business_name", "Brokerage name")}${field("logo_url", "Brokerage logo URL", "url", 2048)}</div><p class="small-note">Use a direct HTTPS image link. Your logo appears beside the brokerage name without cropping.</p><label for="color">Brand color</label><div class="color-options">${["#214d3b", "#24405f", "#a1442c", "#30312e"].map((c) => `<button class="swatch" type="button" style="--swatch:${c}" data-action="color" data-color="${c}" aria-label="Use ${c}" aria-pressed="${s.color.toLowerCase() === c}"></button>`).join("")}<input type="color" name="color" id="color" value="${esc(s.color)}" aria-label="Custom brand color"></div></section><section class="form-section"><h2>Your contact details</h2><p class="muted">Give visitors a way to reach you after they stop by.</p><div class="pair">${field("host_name", "Agent name")}${field("agent_license", "Agent license number", "text", 64)}</div>${field("agent_photo_url", "Agent photo URL", "url", 2048)}<p class="small-note">Use a direct HTTPS image link. A square headshot works best; it appears in a circle beside your name.</p><div class="pair">${field("contact_email", "Agent email", "email", 254)}${field("agent_phone", "Agent phone", "tel", 40)}</div></section><section class="form-section"><h2>Your broker</h2><p class="muted">Add the broker details you want displayed on your check-in page.</p><div class="pair">${field("broker_name", "Broker name")}${field("broker_license", "Broker license number", "text", 64)}</div><div class="pair">${field("broker_email", "Broker email", "email", 254)}${field("broker_phone", "Broker phone", "tel", 40)}</div></section><section class="form-section"><h2>Visitor form</h2><p class="muted">Keep it short. Choose the extra questions that help.</p>${toggle("ask_phone", "Ask for a phone number")}${toggle("ask_timeline", "Ask about buying timeline")}${toggle("ask_agent", "Ask about agent representation")}<p class="small-note">These answers are optional. Follow-up permission is always optional and starts unchecked.</p></section><section class="form-section"><h2>Your welcome</h2><p class="muted">A little hospitality, before they walk through the door.</p>${toggle("show_location", "Show city and state beside the address")}<div class="stack"><label for="welcome">Welcome message<textarea id="welcome" name="welcome" maxlength="300">${esc(s.welcome)}</textarea></label><label for="privacy_note">How visitor details are used<textarea id="privacy_note" name="privacy_note" maxlength="600" required>${esc(s.privacy_note)}</textarea></label></div></section><footer class="settings-actions"><p class="form-error" aria-live="polite"></p><span id="saved-state" class="success-line" role="status">Saved</span><button class="btn primary hidden" id="save-settings" type="submit">Save changes</button></footer></form><aside class="preview-column"><h3>Live preview</h3><p>Example of your visitor’s separate check-in page.</p><div class="preview" id="brand-preview">${preview(s)}</div></aside></div>`,
    "/customize",
  );
}
function readSettings() {
  const form = $("#settings-form");
  const f = new FormData(form);
  const s = { ...data.settings };
  for (const k of Object.keys(s))
    s[k] = typeof s[k] === "boolean" ? f.has(k) : String(f.get(k) ?? "");
  return s;
}
function settingsChanged() {
  const s = readSettings();
  $("#brand-preview").innerHTML = preview(s);
  brandColor(s.color, $("#brand-preview"));
  $("#saved-state").textContent = "Unsaved changes";
  $("#save-settings").classList.remove("hidden");
  document
    .querySelectorAll(".swatch")
    .forEach((el) =>
      el.setAttribute("aria-pressed", String(el.dataset.color === s.color)),
    );
}
function localDate(value) {
  const d = new Date(value);
  return new Date(d.getTime() - d.getTimezoneOffset() * 60000)
    .toISOString()
    .slice(0, 16);
}
function houseDialog(id) {
  const h = data.houses.find((h) => h.id === id);
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const dialog = document.createElement("dialog");
  dialog.id = "house-dialog";
  dialog.setAttribute("aria-labelledby", "house-dialog-title");
  dialog.innerHTML = `<div class="section-head"><h2 id="house-dialog-title">${h ? "Edit open house" : "Open a new door."}</h2><button class="icon-button" data-action="close-dialog" aria-label="Close">${icon("close")}</button></div><form id="house-form" class="dialog-form" ${id ? `data-id="${id}"` : ""}><div class="dialog-fields stack">${input("address", "Property address", h?.address || "", { required: true, max: 160, placeholder: "248 Olive Avenue" })}${input("location", "City and state", h?.location || "", { required: true, max: 160, placeholder: "Oakland, CA" })}<div class="pair">${input("starts_at", "Starts", h ? localDate(h.starts_at) : "", { type: "datetime-local", required: true })}${input("ends_at", "Ends", h ? localDate(h.ends_at) : "", { type: "datetime-local", required: true })}</div><p class="small-note">Times are entered in ${esc(zone)}.</p>${input("photo_url", optional("Property photo URL"), h?.photo_url === "/assets/sample-house.png" ? "" : h?.photo_url || "", { type: "url", max: 2048, placeholder: "https://…" })}<label class="choice"><input name="sample_photo" type="checkbox" ${h?.photo_url === "/assets/sample-house.png" ? "checked" : ""}>Use the illustrative sample photo</label><label for="house-note">${optional("A note for visitors")}<textarea name="note" id="house-note" maxlength="1000" placeholder="Please enter through the front door.">${esc(h?.note || "")}</textarea></label><label for="house-status">Check-in availability<select id="house-status" name="status"><option value="open" ${!h || h.status === "open" ? "selected" : ""}>Open — accept visitors</option><option value="draft" ${h?.status === "draft" ? "selected" : ""}>Draft — visible only to you</option><option value="closed" ${h?.status === "closed" ? "selected" : ""}>Closed — show property and contact details</option></select></label><p class="small-note">You control when check-in opens and closes.</p></div><div class="row actions"><p class="form-error" aria-live="polite"></p><button type="button" class="btn" data-action="close-dialog">Cancel</button><span class="spacer"></span><button class="btn primary" type="submit">${h ? "Save changes" : "Create open house"}</button></div></form>`;
  document.body.append(dialog);
  dialog.showModal();
  dialog.addEventListener("close", () => dialog.remove());
}
async function loadHost() {
  if (document.hidden) return;
  const epoch = ++hostEpoch;
  hostRequest?.abort();
  hostRequest = new AbortController();
  try {
    const fresh = await api("/api/admin/dashboard", { signal: hostRequest.signal });
    if (epoch !== hostEpoch || document.hidden) return;
    data = fresh;
    const remembered = localStorage.getItem("houseworks-house");
    selected = selected || remembered;
    brandColor(data.settings.color);
    document.title = "Open Houseworks · Your workspace";
    app.innerHTML =
      location.pathname === "/customize"
        ? customize()
        : location.pathname === "/visitors"
          ? allVisitors()
          : dashboard();
    restoreHostDraft();
  } catch (error) {
    if (epoch !== hostEpoch || document.hidden) return;
    data = null;
    if (error.status === 401) hostDraft = null;
    if (error.status === 401) loginPage();
    else loginPage(error.message);
  }
}
function hideHost(preserveDraft = true) {
  // Retain only editable host fields in memory. Never retain the visitor payload
  // or private DOM while a tab is hidden or a kiosk is taking over the browser.
  const form = $("#house-form") || $("#settings-form");
  if (!preserveDraft) hostDraft = null;
  else if (form) {
    hostDraft = {
      type: form.id,
      id: form.dataset.id,
      dirty: form.id === "settings-form" && !$("#save-settings").classList.contains("hidden"),
      fields: [...form.elements].filter((el) => el.name).map((el) => [el.name, el.type === "checkbox" ? el.checked : el.value]),
    };
  }
  ++hostEpoch;
  hostRequest?.abort();
  $("#house-dialog")?.remove();
  app.replaceChildren();
  data = null;
}
function restoreHostDraft() {
  const draft = hostDraft;
  hostDraft = null;
  if (!draft) return;
  if (draft.type === "house-form") {
    if (draft.id && !data.houses.some((h) => h.id === draft.id)) return;
    houseDialog(draft.id);
  }
  const form = document.getElementById(draft.type);
  if (!form) return;
  for (const [name, value] of draft.fields) {
    const field = form.elements.namedItem(name);
    if (field) {
      if (typeof value === "boolean") field.checked = value;
      else field.value = value;
    }
  }
  if (draft.dirty) settingsChanged();
}
function hostSignedOut() {
  hideHost(false);
  if (!document.hidden) loginPage();
}
document.addEventListener("submit", async (event) => {
  const form = event.target;
  if (
    !["login-form", "guest-form", "house-form", "settings-form"].includes(
      form.id,
    )
  )
    return;
  event.preventDefault();
  const button =
    form.id === "settings-form"
      ? $("#save-settings")
      : $("button[type=submit]", form);
  button.disabled = true;
  $(".form-error", form).textContent = "";
  const epoch = hostEpoch;
  try {
    const values = Object.fromEntries(new FormData(form));
    if (form.id === "login-form") {
      await api("/api/login", { method: "POST", body: JSON.stringify(values) });
      await loadHost();
    }
    if (form.id === "guest-form") {
      kiosk?.submitting(true);
      values.follow_up = new FormData(form).has("follow_up");
      await api(`/api/public/houses/${data.house.id}`, {
        method: "POST",
        body: JSON.stringify(values),
        signal: AbortSignal.timeout(20000),
      });
      if (!form.isConnected) return;
      form.reset();
      form.outerHTML = `<section class="thank-you" tabindex="-1"><div class="tick" aria-hidden="true">✓</div><h2>You’re checked in.</h2><p>Thanks for coming by. Enjoy your visit.</p><button class="btn" data-action="${kiosk ? "kiosk-reset" : "reload"}">Check in another visitor</button>${kiosk ? '<p class="small-note">Ready for the next visitor in <span id="kiosk-countdown">8</span> seconds.</p>' : ""}</section>`;
      $(".thank-you").focus();
      kiosk?.confirmed();
    }
    if (form.id === "settings-form") {
      const settings = await api("/api/admin/settings", {
        method: "PUT",
        body: JSON.stringify(readSettings()),
      });
      if (epoch !== hostEpoch || !form.isConnected) return;
      data.settings = settings;
      brandColor(data.settings.color);
      $("#saved-state").textContent = "Saved";
      $("#save-settings").classList.add("hidden");
      toast("Your changes are saved.");
    }
    if (form.id === "house-form") {
      values.starts_at = new Date(values.starts_at).toISOString();
      values.ends_at = new Date(values.ends_at).toISOString();
      values.timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
      if (values.sample_photo) values.photo_url = "/assets/sample-house.png";
      delete values.sample_photo;
      const id = form.dataset.id;
      const h = await api(`/api/admin/houses${id ? `/${id}` : ""}`, {
        method: id ? "PUT" : "POST",
        body: JSON.stringify(values),
      });
      if (epoch !== hostEpoch || !form.isConnected) return;
      selected = h.id;
      localStorage.setItem("houseworks-house", selected);
      $("#house-dialog").close();
      await loadHost();
      toast(id ? "Open house updated." : "Your open house is ready.");
    }
  } catch (error) {
    if (form.isConnected)
      errorMessage(
        error.name === "TimeoutError"
          ? new Error(
              "We couldn’t confirm your check-in. Please try again; repeating it won’t create a duplicate.",
            )
          : error,
        form,
      );
    kiosk?.submitting(false);
  } finally {
    button.disabled = false;
  }
});
document.addEventListener("click", async (event) => {
  const target = event.target.closest("[data-action]");
  if (!target) return;
  const action = target.dataset.action;
  try {
    if (action === "new-house") houseDialog();
    if (action === "edit-house") houseDialog(target.dataset.id);
    if (action === "close-dialog") $("#house-dialog").close();
    if (action === "print") window.print();
    if (action === "reload") location.reload();
    if (action === "refresh") {
      await loadHost();
      toast("Up to date.");
    }
    if (action === "logout") {
      hostSignedOut();
      announceSignout();
      await api("/api/logout", { method: "POST" });
      announceSignout();
      location.href = "/";
    }
    if (action === "color") {
      $("#color").value = target.dataset.color;
      settingsChanged();
    }
    if (action === "publish-house") {
      const h = data.houses.find((h) => h.id === target.dataset.id);
      await api(`/api/admin/houses/${h.id}`, {
        method: "PUT",
        body: JSON.stringify({ ...h, status: "open" }),
      });
      await loadHost();
      toast("Your check-in page is open.");
    }
    if (action === "delete-visitor") {
      const v = data.visitors.find((v) => v.id === target.dataset.id);
      if (
        !confirm(
          `Delete ${v.name}’s registration? This permanently removes their contact details and follow-up choice.`,
        )
      )
        return;
      await api(`/api/admin/visitors/${v.id}`, { method: "DELETE" });
      await loadHost();
      toast("Visitor details deleted.");
    }
  } catch (error) {
    toast(error.message);
  }
});
// Image errors do not bubble. Capture them so a missing optional image leaves
// the host details readable without a broken-image placeholder.
document.addEventListener("error", (event) => {
  if (event.target instanceof HTMLImageElement && event.target.classList.contains("contact-image")) {
    event.target.hidden = true;
  }
}, true);
document.addEventListener("input", (event) => {
  if (event.target.closest("#settings-form")) settingsChanged();
  if (event.target.id === "visitor-search") {
    let visible = 0;
    document.querySelectorAll("[data-visitor]").forEach((row) => {
      const show = row.dataset.search.includes(
        event.target.value.trim().toLowerCase(),
      );
      row.classList.toggle("hidden", !show);
      if (show) visible++;
    });
    $("#no-matches")?.classList.toggle("hidden", visible > 0);
  }
});
document.addEventListener("change", (event) => {
  if (event.target.id === "house-selector") {
    selected = event.target.value;
    localStorage.setItem("houseworks-house", selected);
    app.innerHTML = dashboard();
  }
});
window.addEventListener("beforeunload", (event) => {
  if (
    $("#save-settings") &&
    !$("#save-settings").classList.contains("hidden")
  ) {
    event.preventDefault();
    event.returnValue = "";
  }
});
const publicRoute = location.pathname.match(
  /^\/(visit|sign)\/([0-9a-f-]{36})$/i,
);
async function startKiosk() {
  kiosk = createKioskController({
    reset: () => loadGuest(kioskRoute[1], false, true),
    notice: toast,
  });
  try {
    announceSignout();
    await api("/api/kiosk/start", { method: "POST" });
    announceSignout();
    await loadGuest(kioskRoute[1], false, true);
  } catch {
    app.innerHTML =
      '<main id="main" class="loading"><h1>Unable to start check-in.</h1><p>We couldn’t securely sign this device out. Please try again.</p><button class="btn" data-action="reload">Try again</button></main>';
  }
}
if (kioskRoute) startKiosk();
else if (publicRoute) loadGuest(publicRoute[2], publicRoute[1] === "sign");
else {
  loadHost();
  // Cached pages and other host tabs must recheck access after kiosk launch.
  window.addEventListener("pagehide", () => hideHost());
  window.addEventListener("pageshow", (event) => {
    if (event.persisted) loadHost();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) hideHost();
    else loadHost();
  });
  sessionChannel?.addEventListener("message", (event) => {
    if (event.data === "signed-out") hostSignedOut();
  });
  window.addEventListener("storage", (event) => {
    if (event.key === "houseworks-signout") hostSignedOut();
  });
}
