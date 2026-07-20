document.addEventListener("htmx:configRequest", function (event) {
  const method = String(event.detail.verb || "GET").toUpperCase();
  if (!["POST", "PUT", "PATCH", "DELETE"].includes(method)) return;
  const target = new URL(event.detail.path, window.location.origin);
  if (target.origin !== window.location.origin) return;
  const token = document.querySelector('meta[name="csrf-token"]')?.content;
  if (token) event.detail.headers["X-CSRF-Token"] = token;
});

let lastRequestFocus = null;

function requestControl(element) {
  if (!element) return null;
  if (element.matches?.("button, input[type='submit']")) return element;
  return element.querySelector?.("button[type='submit'], input[type='submit']") || null;
}

function accessibleName(element) {
  return (element?.getAttribute("aria-label") || element?.textContent || element?.value || "")
    .trim();
}

function announce(message) {
  const region = document.getElementById("notifications");
  if (!region) return;
  region.replaceChildren();
  const notification = document.createElement("p");
  notification.className = "notification notification--error";
  notification.setAttribute("role", "alert");
  notification.textContent = message;
  region.append(notification);
}

document.addEventListener("htmx:beforeRequest", function (event) {
  const control = requestControl(event.detail.elt);
  const target = event.detail.target;
  if (target) {
    lastRequestFocus = {
      action: control?.form?.getAttribute("action") || "",
      name: accessibleName(control),
      targetId: target.id || "",
    };
    target.setAttribute("aria-busy", "true");
  }
  if (control) control.setAttribute("aria-busy", "true");
});

document.addEventListener("htmx:afterRequest", function (event) {
  event.detail.target?.removeAttribute("aria-busy");
  requestControl(event.detail.elt)?.removeAttribute("aria-busy");
});

function announceUncertainMutation() {
  announce("The request did not finish. Reload the latest workshop record before trying again.");
}

document.addEventListener("htmx:sendError", announceUncertainMutation);
document.addEventListener("htmx:timeout", announceUncertainMutation);

document.addEventListener("htmx:beforeSwap", function (event) {
  const loginResponse = event.detail.target?.id === "login-region";
  const swappableStatus = [401, 422, 429, 503].includes(event.detail.xhr.status);
  if (loginResponse && swappableStatus) {
    event.detail.shouldSwap = true;
    event.detail.isError = false;
  }

  const customerResponse = [
    "customer-form",
    "customer-list-content",
    "customer-detail",
    "vehicle-form",
    "attachment-form",
    "knowledge-form",
    "invoice-form",
    "invoice-line-form",
    "invoice-line-region",
    "main-content",
  ]
    .includes(event.detail.target?.id);
  if (customerResponse && [409, 422].includes(event.detail.xhr.status)) {
    event.detail.shouldSwap = true;
    event.detail.isError = false;
  }
});

document.addEventListener("htmx:afterSettle", function (event) {
  const eventTarget = event.detail.target;
  if (!eventTarget) return;
  const target = eventTarget.id
    ? document.getElementById(eventTarget.id) || eventTarget
    : eventTarget;
  target.removeAttribute("aria-busy");

  if (target.id === "main-content") {
    const heading = target.querySelector("h1");
    if (heading?.textContent?.trim()) document.title = `${heading.textContent.trim()} · Pipauto`;
  }

  const invalid = target.querySelector?.('[aria-invalid="true"]');
  if (invalid) {
    invalid.focus({ preventScroll: true });
    invalid.scrollIntoView({ block: "center" });
    return;
  }

  const previous = lastRequestFocus?.targetId === target.id ? lastRequestFocus : null;
  if (previous?.action) {
    const forms = Array.from(target.querySelectorAll?.("form") || []);
    const matchingForm = forms.find(
      (form) => form.getAttribute("action") === previous.action,
    );
    const matching = Array.from(matchingForm?.querySelectorAll("button") || []).find(
      (candidate) => accessibleName(candidate) === previous.name,
    );
    if (matching) {
      matching.focus({ preventScroll: true });
      return;
    }
  }

  if (!target.hasAttribute("tabindex")) target.setAttribute("tabindex", "-1");
  target.focus({ preventScroll: true });
});

document.addEventListener("focusin", function (event) {
  const customer = event.target.closest("[data-invoice-customer]");
  if (customer) customer.dataset.previousValue = customer.value;
});

document.addEventListener("change", function (event) {
  const customer = event.target.closest("[data-invoice-customer]");
  if (!customer) return;
  const form = customer.closest("[data-invoice-relationships]");
  const vehicle = form?.querySelector("[data-invoice-vehicle]");
  const intervention = form?.querySelector("[data-invoice-intervention]");
  const selectedVehicle = vehicle?.selectedOptions[0];
  const incompatibleVehicle = Boolean(
    selectedVehicle?.value && selectedVehicle.dataset.ownerId !== customer.value,
  );
  if (!incompatibleVehicle) return;
  const clear = window.confirm(
    "Changing the customer clears the selected vehicle and intervention. Continue?",
  );
  if (clear) {
    vehicle.value = "";
    intervention.value = "";
  } else {
    customer.value = customer.dataset.previousValue || "";
  }
});

document.addEventListener("click", function (event) {
  const removeTag = event.target.closest("[data-remove-tag]");
  if (removeTag) {
    const editor = removeTag.closest("[data-tag-editor]");
    const chip = removeTag.closest("[data-tag-chip]");
    const textarea = editor?.querySelector('textarea[name="tags"]');
    if (!chip || !textarea) return;
    const chips = Array.from(editor.querySelectorAll("[data-tag-chip]"));
    const index = chips.indexOf(chip);
    const tags = textarea.value.split(/\r?\n/);
    if (index >= 0) tags.splice(index, 1);
    textarea.value = tags.join("\n");
    chip.remove();
    textarea.dispatchEvent(new Event("change", { bubbles: true }));
    return;
  }
  const openButton = event.target.closest("[data-dialog-open]");
  if (openButton) {
    const dialog = document.getElementById(openButton.dataset.dialogOpen);
    if (dialog?.showModal) {
      dialog.returnFocus = openButton;
      dialog.showModal();
    }
    return;
  }
  const closeButton = event.target.closest("[data-dialog-close]");
  if (closeButton) closeButton.closest("dialog")?.close();
});

document.addEventListener("close", function (event) {
  if (event.target.matches?.("dialog")) event.target.returnFocus?.focus();
}, true);

document.addEventListener("keydown", function (event) {
  if (event.key !== "Escape") return;
  document.querySelectorAll(".more-menu[open]").forEach(function (menu) {
    menu.removeAttribute("open");
    menu.querySelector("summary")?.focus();
  });
});
