// Shared-device behavior. Visitor details never enter browser storage.
export function createKioskController({ reset, notice }) {
  let deadline = 0;
  let confirmationUntil = 0;
  let submitting = false;
  let resetting = false;
  const form = () => document.querySelector("#guest-form");
  const warning = () => document.querySelector("#kiosk-idle");
  const hasDetails = () =>
    [...(form()?.elements || [])].some((el) =>
      ["checkbox", "radio"].includes(el.type)
        ? el.checked
        : el.name && el.name !== "website" && el.value.trim(),
    );
  function hideWarning() {
    if (warning()) warning().hidden = true;
  }
  function touch(event) {
    if (event && !event.target.closest("#guest-form, #kiosk-idle")) return;
    if (!submitting && !confirmationUntil) {
      deadline = hasDetails() ? Date.now() + 120_000 : 0;
      hideWarning();
    }
  }
  async function startOver() {
    if (submitting || resetting) return;
    resetting = true;
    deadline = confirmationUntil = 0;
    form()?.reset();
    hideWarning();
    try {
      await reset();
      window.scrollTo({ top: 0, behavior: "instant" });
    } finally {
      resetting = false;
    }
  }
  function fullscreenState() {
    const active = !!(
      document.fullscreenElement || document.webkitFullscreenElement
    );
    const button = document.querySelector('[data-action="fullscreen"]');
    if (!button) return;
    const label = active ? "Exit fullscreen" : "Enter fullscreen";
    button.setAttribute("aria-label", label);
    button.setAttribute("title", label);
    button.setAttribute("aria-pressed", String(active));
  }
  async function fullscreen() {
    try {
      if (document.fullscreenElement || document.webkitFullscreenElement) {
        const exit = document.exitFullscreen || document.webkitExitFullscreen;
        await exit.call(document);
      } else {
        const enter =
          document.documentElement.requestFullscreen ||
          document.documentElement.webkitRequestFullscreen;
        if (!enter) {
          notice(
            "Use your browser’s fullscreen mode, or add this page to your Home Screen.",
          );
          return;
        }
        await enter.call(document.documentElement);
      }
      fullscreenState();
    } catch {
      notice(
        "Fullscreen is unavailable here. Try your browser’s fullscreen control or Home Screen mode.",
      );
    }
  }
  for (const type of ["input", "change", "pointerdown", "keydown"]) {
    document.addEventListener(type, touch);
  }
  document.addEventListener("click", (event) => {
    const action = event.target.closest("[data-action]")?.dataset.action;
    if (action === "kiosk-reset") startOver();
    if (action === "kiosk-continue") touch();
    if (action === "fullscreen") fullscreen();
  });
  document.addEventListener("fullscreenchange", fullscreenState);
  document.addEventListener("webkitfullscreenchange", fullscreenState);
  // Timer uses wall time so suspension and background throttling cannot extend
  // the life of an abandoned visitor's details indefinitely.
  function tick() {
    if (submitting || resetting) return;
    if (confirmationUntil) {
      const seconds = Math.max(
        0,
        Math.ceil((confirmationUntil - Date.now()) / 1000),
      );
      const counter = document.querySelector("#kiosk-countdown");
      if (counter) counter.textContent = String(seconds);
      if (!seconds) startOver();
      return;
    }
    if (!hasDetails()) {
      deadline = 0;
      hideWarning();
      return;
    }
    if (!deadline) deadline = Date.now() + 120_000;
    const remaining = deadline - Date.now();
    if (remaining <= 0) startOver();
    else if (remaining <= 30_000 && warning()) warning().hidden = false;
  }
  setInterval(tick, 1000);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) tick();
  });
  window.addEventListener("pagehide", () => {
    form()?.reset();
    deadline = 0;
  });
  window.addEventListener("pageshow", (event) => {
    if (event.persisted) startOver();
  });
  return {
    rendered() {
      deadline = confirmationUntil = 0;
      fullscreenState();
    },
    submitting(value) {
      submitting = value;
      document
        .querySelectorAll('[data-action="kiosk-reset"]')
        .forEach((button) => {
          button.disabled = value;
        });
      if (value) hideWarning();
      else touch();
    },
    confirmed() {
      submitting = false;
      deadline = 0;
      confirmationUntil = Date.now() + 8000;
      document
        .querySelectorAll('[data-action="kiosk-reset"]')
        .forEach((button) => {
          button.disabled = false;
        });
      document.activeElement?.blur();
      document.querySelector(".thank-you")?.focus({ preventScroll: true });
    },
  };
}
