// Carries out one of the site's commands in the page showing (keys.rs): the
// site's own `window.oeeeCommand` (toolbar.jinja in oeee-cafe/web), or else a
// plain load of the page it would go to, when there is one. A plain load, so
// a page holding a drawing asks first. Called with the command's name and
// that page's address, or null.
(function (command, fallback) {
  if (window.oeeeCommand && window.oeeeCommand(command)) return;
  if (fallback) location.href = fallback;
})
