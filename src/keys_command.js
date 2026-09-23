// Carries out one of the site's commands in the page showing (keys.rs): the
// site's own `window.oeeeApp.command` (toolbar.jinja in oeee-cafe/web). A
// page without the toolbar has none, and the key does nothing there. Called
// with the command's name.
(function (command) {
  if (window.oeeeApp && window.oeeeApp.command) window.oeeeApp.command(command);
})
