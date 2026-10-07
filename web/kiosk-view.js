export function kioskView(h, s, { esc, icon, form, contact, dateLabel }) {
  const image = h.photo_url
    ? '<img class="kiosk-photo" src="' +
      esc(h.photo_url) +
      '" alt="' +
      esc(h.address) +
      '">'
    : "";
  const logo = s.logo_url
    ? '<img class="guest-logo" src="' +
      esc(s.logo_url) +
      '" alt="' +
      esc(s.business_name || "Host logo") +
      '">'
    : "";
  const brand = s.business_name
    ? '<p class="guest-brand">' + esc(s.business_name) + "</p>"
    : "";
  const note = h.note ? '<p class="small-note">' + esc(h.note) + "</p>" : "";
  const content =
    h.status === "open"
      ? form
      : '<section class="thank-you"><h2>Check-in has closed.</h2><p>Thanks for your interest. You can reach the host using the details below.</p></section>';
  return [
    '<main id="main" class="kiosk">',
    '<section class="kiosk-property">' + image,
    '<div class="kiosk-property-copy">' + logo + brand,
    "<h1>Welcome to <span>" + esc(h.address) + "</span></h1>",
    '<p class="place">' +
      esc(h.location) +
      '</p><p class="date">' +
      esc(dateLabel(h)) +
      "</p>",
    note + "</div></section>",
    '<section class="kiosk-entry">',
    '<div class="kiosk-controls"><button class="btn quiet" data-action="kiosk-reset">Start over</button>',
    '<button class="icon-button" data-action="fullscreen" aria-label="Enter fullscreen" title="Enter fullscreen" aria-pressed="false">' +
      icon("expand") +
      "</button></div>",
    '<header class="kiosk-entry-header"><h2>Check in here</h2>',
    '<p class="kiosk-welcome">' +
      esc(s.welcome || "Make yourself at home.") +
      "</p></header>",
    '<div id="kiosk-form-slot">' + content + "</div></section>",
    '<aside class="kiosk-qr" aria-label="Check in on your own device"><div><h2>Use your own phone</h2><p>Scan to check in. No app needed.</p></div>',
    '<img src="/api/public/houses/' +
      h.id +
      '/qr.svg" alt="QR code to open this property’s check-in page on your phone"></aside>',
    '<div class="kiosk-contacts">' + contact + "</div>",
    '<footer class="kiosk-bottom"><p>Open Houseworks</p><a href="/">Host sign-in</a></footer>',
    '</main><aside id="kiosk-idle" class="kiosk-idle" hidden><p role="status">Still there? This form will clear in 30 seconds to protect your details.</p>',
    '<div class="row"><button class="btn" data-action="kiosk-continue">I’m still here</button><button class="btn quiet" data-action="kiosk-reset">Clear form</button></div></aside>',
  ].join("");
}
