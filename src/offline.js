// Runs in every page of the site: a page navigation htmx could not complete
// is made again as an ordinary one, which the window sees fail (offline.rs).
(function () {
  var GATEWAY = __GATEWAY__;
  // A page, as htmx fetches one: the whole body, by GET -- a boosted link
  // or search. Anything smaller is a part of the page, and its failure is
  // the page's to show.
  function page(ctx) {
    return !!ctx && ctx.target === document.body && !!ctx.request &&
      String(ctx.request.method).toUpperCase() === "GET";
  }
  function again(ctx) {
    location.assign(ctx.request.action);
  }
  document.addEventListener("htmx:before:swap", function (event) {
    var ctx = event.detail && event.detail.ctx;
    if (!page(ctx) || !ctx.response || GATEWAY.indexOf(ctx.response.status) < 0) return;
    event.preventDefault();
    again(ctx);
  });
  document.addEventListener("htmx:error", function (event) {
    var ctx = event.detail && event.detail.ctx;
    if (!page(ctx) || ctx.response) return;
    again(ctx);
  });
})();
