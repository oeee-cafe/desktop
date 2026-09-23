// Signing in through the player's browser and taking the answer back (handoff.rs).
//
// Apple and Google cannot be signed in with here. Google refuses its own pages inside an
// embedded web view, and Apple has no sheet to open on Windows; and either page opened in
// the player's browser answers there, where the cookies are not this window's. So the site
// hands the sign-in out and takes it back: this asks the site to start a handoff, the app
// opens the browser at the URL it gets, and this asks the site until the browser has
// finished -- at which point the site signs *this* window in (src/handoff.rs in
// oeee-cafe/web).
//
// Every request is made here, in the page, so each carries the page's cookie and origin.
// The app's whole part is opening a browser: it never holds the session, and never sees
// the secret that claims the handoff.
//
// Defined in every page the window loads, as the bridge is, and for the same reason: the
// window only ever holds the site and the bundled loader, and a page Tauri has given no
// IPC to is left without this rather than given one that cannot carry.
(function () {
  if (!window.__TAURI_INTERNALS__) return;

  // Asks the app to open `url` in the browser. The app checks it is the site's own.
  function openInBrowser(url) {
    window.__TAURI_INTERNALS__
      .invoke("plugin:event|emit", { event: "oeee-handoff", payload: url })
      .catch(function () {});
  }

  // The handoff under way: its id, its secret, and where it is going afterwards.
  var pending = null;
  var asking = null;
  // Whether an ask is in flight. Two at once can spend the handoff between them: one gets
  // the sign-in and the other gets "there is no such handoff", and whichever lands second
  // decides what the page does.
  var inFlight = false;

  // How often to ask whether the browser has finished, and how long to keep asking. The
  // site forgets a handoff after fifteen minutes, so there is nothing to find after that.
  var ASK_EVERY = 2000;
  var GIVE_UP_AFTER = 15 * 60 * 1000;

  function stop() {
    if (asking) {
      clearInterval(asking);
      asking = null;
    }
    pending = null;
    inFlight = false;
  }

  function form(fields) {
    var body = new URLSearchParams();
    for (var name in fields) {
      if (fields[name]) body.set(name, fields[name]);
    }
    return body.toString();
  }

  function post(path, fields) {
    return fetch(path, {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: form(fields)
    }).then(function (response) {
      return response.ok ? response.json() : null;
    });
  }

  // Asks the site whether the browser has finished, and acts on what it says.
  function ask(confirmed) {
    if (!pending || inFlight) return;
    inFlight = true;
    var fields = { id: pending.id, secret: pending.secret };
    if (confirmed) fields.confirm = "1";
    post("/auth/handoff/claim", fields)
      .then(function (answer) {
        inFlight = false;
        // The site could not answer: nothing has been decided, so keep asking rather than
        // giving up on a sign-in that may well have taken.
        if (!answer || !pending) return;
        if (answer.status === "waiting" || answer.status === "failed") return;
        if (answer.status === "confirm") {
          // Linking the browser's account to the one signed in here. The site words the
          // question, because it is the one that knows the reader's language; the app
          // draws it as its own dialog (dialogs.rs).
          if (window.confirm(answer.message)) {
            ask(true);
          } else {
            stop();
          }
          return;
        }
        if (answer.status === "ready") {
          var next = answer.next || "/";
          stop();
          // Replaced, not pushed: the page signed in from is not left behind the one it
          // lands on, so Back does not return to a sign-in form for an account already
          // signed in.
          location.replace(next);
          return;
        }
        // "unknown", or anything a later site says that this does not know.
        stop();
      })
      .catch(function () {
        inFlight = false;
      });
  }

  window.oeeeHandoffAuth = {
    // Starts a sign-in with `provider` in the browser, going on to `next` afterwards.
    // The app calls this when it stops the link to /auth/<provider>.
    begin: function (provider, next) {
      if (pending) return;
      post("/auth/handoff/start", { provider: provider, next: next })
        .then(function (started) {
          if (!started || !started.id || !started.secret || !started.url) {
            stop();
            return;
          }
          pending = started;
          openInBrowser(started.url);
          asking = setInterval(function () {
            ask(false);
          }, ASK_EVERY);
          setTimeout(function () {
            if (pending) stop();
          }, GIVE_UP_AFTER);
        })
        .catch(stop);
    },

    // The window is in front again, so somebody may have just come back from the browser:
    // ask now rather than waiting for the next turn of the clock.
    resume: function () {
      if (pending) ask(false);
    }
  };
})();
