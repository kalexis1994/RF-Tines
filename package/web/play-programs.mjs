import { validateProgram } from "./rftines-format.mjs";

const PROTOCOL = "rackforge.plugin.web@1";
const PLUGIN_ID = "org.rackforge.rftines";
const REQUEST_TIMEOUT_MS = 10_000;
const SETTLE_TIMEOUT_MS = 10_000;
const openButton = document.querySelector("#save-program-open");
const dialog = document.querySelector("#save-program-dialog");
const form = document.querySelector("#save-program-form");
const nameInput = document.querySelector("#save-program-name");
const submitButton = document.querySelector("#save-program-submit");
const cancelButton = document.querySelector("#save-program-cancel");
const closeButton = document.querySelector("#save-program-close");
const dialogStatus = document.querySelector("#save-program-status");
const feedback = document.querySelector("#save-program-feedback");
const hostStatus = document.querySelector("#status");
// RackForge's <rf-program-select>: it chooses programs on its own; this page
// only holds it still while a program is being saved, and reports refusals.
const programSelector = document.querySelector("#program-selector");
const programError = document.querySelector("#program-error");

let context = null;
let requestSerial = 0;
let busy = false;
const requests = new Map();
const contextWaiters = new Set();

function connected() {
  return context?.instance?.plugin_id === PLUGIN_ID;
}

function controlsReady() {
  return hostStatus.dataset.ready === "true";
}

function render() {
  const available = connected() && controlsReady() && !busy && !context?.program_draft;
  openButton.disabled = !available;
  programSelector.toggleAttribute("disabled", busy || Boolean(context?.program_draft));
  nameInput.disabled = busy;
  cancelButton.disabled = busy;
  closeButton.disabled = busy;
  submitButton.disabled = busy || !nameInput.value.trim();
  form.setAttribute("aria-busy", busy ? "true" : "false");
}

function call(method, params = {}) {
  const requestId = `rftines-play-program-${++requestSerial}`;
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      requests.delete(requestId);
      reject(new Error("RackForge did not answer in time."));
    }, REQUEST_TIMEOUT_MS);
    requests.set(requestId, { resolve, reject, timer });
    parent.postMessage(
      { protocol: PROTOCOL, kind: "request", request_id: requestId, method, params },
      location.origin,
    );
  });
}

function waitForContext(predicate) {
  if (context && predicate(context)) return Promise.resolve(context);
  return new Promise((resolve, reject) => {
    const waiter = { predicate, resolve, reject, timer: 0 };
    waiter.timer = window.setTimeout(() => {
      contextWaiters.delete(waiter);
      reject(new Error("RackForge did not finish saving the program in time."));
    }, REQUEST_TIMEOUT_MS);
    contextWaiters.add(waiter);
  });
}

function publishContext(next) {
  context = next;
  for (const waiter of [...contextWaiters]) {
    if (waiter.predicate(next)) {
      window.clearTimeout(waiter.timer);
      contextWaiters.delete(waiter);
      waiter.resolve(next);
    }
  }
  render();
}

function displayedParameters() {
  const values = new Map();
  for (const control of document.querySelectorAll("[data-rackforge-parameter-index]")) {
    const index = Number(control.dataset.rackforgeParameterIndex);
    const value = Number(control.value);
    if (Number.isInteger(index) && Number.isFinite(value)) values.set(index, value);
  }
  if (values.size !== 15) throw new Error("The RF-Tines controls are not ready yet.");
  return values;
}

function returnedParameters(result) {
  const values = new Map();
  for (const entry of result?.values ?? []) {
    if (Number.isInteger(entry.index) && Number.isFinite(entry.value)) {
      values.set(entry.index, entry.value);
    }
  }
  return values;
}

function sameParameters(expected, actual) {
  if (expected.size !== actual.size) return false;
  return [...expected].every(([index, value]) => Math.abs(value - actual.get(index)) <= 1e-6);
}

function pause(milliseconds) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

async function waitForControlsToSettle() {
  const deadline = performance.now() + SETTLE_TIMEOUT_MS;
  while (performance.now() < deadline) {
    const expected = displayedParameters();
    const actual = returnedParameters(await call("plugin.parameters"));
    if (sameParameters(expected, actual)) return;
    await pause(50);
  }
  throw new Error("The latest control changes have not reached RackForge yet.");
}

async function cancelDraft(draftId) {
  try {
    await call("plugin.cancel_program", { draft_id: draftId });
    await waitForContext((next) => !next.program_draft);
  } catch {
    // Preserve the original failure. RackForge expires abandoned drafts.
  }
}

async function saveProgram() {
  const programName = nameInput.value.trim();
  if (!programName || busy) return;
  busy = true;
  dialogStatus.textContent = "Saving the current controls…";
  feedback.textContent = "";
  render();
  let draftId = null;
  try {
    await waitForControlsToSettle();
    await call("plugin.begin_program_edit", { program_id: null });
    const opened = await waitForContext((candidate) => candidate.program_draft);
    draftId = opened.program_draft.draft_id;
    const document = JSON.parse(opened.program_draft.document_json);
    document.name = programName;
    validateProgram(document);
    const soundId = `custom.${document.id}`;
    await call("plugin.set_program_name", { draft_id: draftId, name: programName });
    await waitForContext((candidate) => {
      if (candidate.program_draft?.draft_id !== draftId) return false;
      try {
        return JSON.parse(candidate.program_draft.document_json).name === programName;
      } catch {
        return false;
      }
    });
    await call("plugin.save_program", { draft_id: draftId });
    await waitForContext(
      (candidate) => !candidate.program_draft && candidate.instance.sounds.some((sound) => sound.id === soundId),
    );
    draftId = null;
    if (context.instance.selected_sound_id !== soundId) {
      await call("plugin.select_sound", { sound_id: soundId });
      await waitForContext((candidate) => candidate.instance.selected_sound_id === soundId);
    }
    nameInput.value = "";
    dialogStatus.textContent = "";
    feedback.textContent = `${programName} saved in your RF-Tines library.`;
    dialog.close();
  } catch (cause) {
    if (draftId !== null) await cancelDraft(draftId);
    dialogStatus.textContent = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy = false;
    render();
  }
}

function openDialog() {
  if (openButton.disabled) return;
  dialogStatus.textContent = "";
  feedback.textContent = "";
  dialog.showModal();
  nameInput.focus();
}

function closeDialog() {
  if (!busy) dialog.close();
}

openButton.addEventListener("click", openDialog);
cancelButton.addEventListener("click", closeDialog);
closeButton.addEventListener("click", closeDialog);
nameInput.addEventListener("input", render);
submitButton.addEventListener("click", () => void saveProgram());
form.addEventListener("submit", (event) => {
  event.preventDefault();
  void saveProgram();
});
dialog.addEventListener("cancel", (event) => {
  if (busy) event.preventDefault();
});
dialog.addEventListener("click", (event) => {
  if (!busy && event.target === dialog) dialog.close();
});

programSelector.addEventListener("rf-program-select", () => {
  programError.textContent = "";
});
programSelector.addEventListener("rf-program-error", () => {
  programError.textContent = "Could not load the program. Please try again.";
});

new MutationObserver(render).observe(hostStatus, { attributes: true, attributeFilter: ["data-ready"] });

window.addEventListener("message", (event) => {
  if (event.source !== parent || event.origin !== location.origin || event.data?.protocol !== PROTOCOL) return;
  if (event.data.kind === "context" && event.data.surface === "play" && event.data.instance?.plugin_id === PLUGIN_ID) {
    publishContext(event.data);
  } else if (event.data.kind === "response" && requests.has(event.data.request_id)) {
    const pending = requests.get(event.data.request_id);
    requests.delete(event.data.request_id);
    window.clearTimeout(pending.timer);
    if (event.data.ok) pending.resolve(event.data.result);
    else pending.reject(new Error(event.data.error || "RackForge rejected the operation."));
  }
});

render();
