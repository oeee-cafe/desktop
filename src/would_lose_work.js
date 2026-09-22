// Whether the page would stop a browser from leaving it: the page's own
// `beforeunload` handlers, asked the way a browser asks them (close_guard.rs).
// An expression, evaluated for its answer.
(function () {
  var event;
  try {
    event = document.createEvent("BeforeUnloadEvent");
    event.initEvent("beforeunload", false, true);
  } catch (_) {
    event = new Event("beforeunload", { cancelable: true });
  }
  window.dispatchEvent(event);
  return event.defaultPrevented ||
    (typeof event.returnValue === "string" && event.returnValue !== "");
})()
