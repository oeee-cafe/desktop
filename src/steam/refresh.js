// Posts a Steam ticket to the site's `/auth/steam/refresh` from the page that
// is showing, without leaving it (steam.rs). Called with the ticket.
(function (ticket) {
  var body = new URLSearchParams();
  body.append("ticket", ticket);
  fetch("/auth/steam/refresh", { method: "POST", body: body, credentials: "same-origin" })
    .catch(function () {});
})
