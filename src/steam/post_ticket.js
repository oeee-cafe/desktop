// Posts a Steam ticket to the site's `/auth/steam` from the page that is
// showing, as a form on it would (steam.rs). Same-origin, so the session
// cookie goes with it and the site can link the ticket's account to the one
// already signed in. Called with the ticket and where to go next, or null.
(function (ticket, next) {
  var form = document.createElement("form");
  form.method = "post";
  form.action = "/auth/steam";
  function field(name, value) {
    var input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = value;
    form.appendChild(input);
  }
  field("ticket", ticket);
  if (next) field("next", next);
  document.body.appendChild(form);
  form.submit();
})
