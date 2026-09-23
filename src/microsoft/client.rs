//! Windows.Services.Store itself, in the Microsoft Store's build: the
//! prices of add-ons, the purchase dialog, and the Microsoft Store ID key
//! that proves what was bought, as `super` describes.
//!
//! A desktop app's `StoreContext` has no window of its own to put the
//! Store's dialogs over, so each one is given the app's window by
//! `IInitializeWithWindow` before it is used. Every call blocks until the
//! Store answers, so each is made off the main thread -- except starting a
//! purchase, which shows a dialog and so is started on the window's own
//! thread and only waited for off it.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Listener, Manager, WebviewWindow};
use windows::core::{Interface, HSTRING};
use windows::Services::Store::{StoreContext, StorePurchaseStatus};
use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, HWND};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::UI::Shell::IInitializeWithWindow;
use windows_collections::IIterable;

use super::{ticket_reply, ticket_script, Ticket, TICKET_EVENT, TICKET_TIMEOUT};
use crate::{store, WINDOW};

/// The product kind the Supporter Pack is in Partner Center: a durable
/// add-on, bought once and kept.
const DURABLE: &str = "Durable";

type Waiting = Arc<Mutex<HashMap<u64, Sender<Option<Ticket>>>>>;

pub struct Microsoft {
    app: AppHandle,
    /// The window's handle, as a number: a handle is not `Send`, and the
    /// Store is asked from other threads.
    window: isize,
    waiting: Waiting,
    next: AtomicU64,
}

/// Whether the app is running as a package, which is the only way the
/// Store knows what it is: unpackaged, every question to it fails.
fn packaged() -> bool {
    let mut length = 0u32;
    unsafe { GetCurrentPackageFullName(&mut length, None) != APPMODEL_ERROR_NO_PACKAGE }
}

/// Starts selling through the Store when this is its package, or says why
/// not and sells nothing.
pub fn start(window: &WebviewWindow) -> Option<Arc<Microsoft>> {
    if !packaged() {
        eprintln!("not running as a package: the Microsoft Store sells nothing here");
        return None;
    }
    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd.0 as isize,
        Err(error) => {
            eprintln!("no window for the Microsoft Store: {error}");
            return None;
        }
    };
    let waiting: Waiting = Arc::default();
    window.app_handle().listen_any(TICKET_EVENT, {
        let waiting = waiting.clone();
        move |event| {
            let Some((id, ticket)) = ticket_reply(event.payload()) else {
                return;
            };
            if let Some(reply) = waiting.lock().unwrap().remove(&id) {
                let _ = reply.send(ticket);
            }
        }
    });
    Some(Arc::new(Microsoft {
        app: window.app_handle().clone(),
        window: hwnd,
        waiting,
        next: AtomicU64::new(1),
    }))
}

impl Microsoft {
    /// The Store, with the app's window to show its dialogs over.
    fn context(&self) -> windows::core::Result<StoreContext> {
        let context = StoreContext::GetDefault()?;
        unsafe {
            context
                .cast::<IInitializeWithWindow>()?
                .Initialize(HWND(self.window as *mut c_void))?;
        }
        Ok(context)
    }

    fn eval(&self, script: String) {
        if let Some(window) = self.app.get_webview_window(WINDOW) {
            let _ = window.eval(script);
        }
    }

    /// Answers the page's `prices` with each add-on's price as the Store
    /// formats it for the player; one the Store does not know is left out,
    /// and the page shows its button without a price.
    pub fn answer_prices(self: &Arc<Self>, products: Vec<String>) {
        if products.is_empty() {
            return;
        }
        let this = self.clone();
        std::thread::spawn(move || match this.prices(&products) {
            Ok(prices) if !prices.is_empty() => this.eval(store::prices_script(&prices)),
            Ok(_) => eprintln!("the Microsoft Store has no price for {products:?}"),
            Err(error) => eprintln!("no prices from the Microsoft Store: {error}"),
        });
    }

    fn prices(&self, products: &[String]) -> windows::core::Result<BTreeMap<String, String>> {
        let context = self.context()?;
        let kinds = IIterable::<HSTRING>::from(vec![HSTRING::from(DURABLE)]);
        let ids = IIterable::<HSTRING>::from(products.iter().map(HSTRING::from).collect::<Vec<_>>());
        let answer = context.GetStoreProductsAsync(&kinds, &ids)?.get()?;
        answer.ExtendedError()?.ok()?;
        let found = answer.Products()?;
        let mut prices = BTreeMap::new();
        for product in products {
            let Ok(item) = found.Lookup(&HSTRING::from(product)) else {
                continue;
            };
            let price = item.Price()?.FormattedPrice()?.to_string();
            if !price.is_empty() {
                prices.insert(product.clone(), price);
            }
        }
        Ok(prices)
    }

    /// Sells an add-on in the Store's own dialog, and hands the page proof
    /// once the player owns it. A player who closes the dialog has bought
    /// nothing, and nothing is said; anything that goes wrong is logged,
    /// and the site's own recheck is left to find what was bought.
    pub fn sell(self: &Arc<Self>, product: String) {
        let this = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = this.buy(&product) {
                eprintln!("could not sell {product:?} through the Microsoft Store: {error}");
            }
        });
    }

    fn buy(&self, product: &str) -> Result<(), String> {
        let context = self.context().map_err(|e| e.to_string())?;

        // Started on the window's thread, where a dialog belongs, and waited
        // for here.
        let (started, purchase) = mpsc::channel();
        let asked = context.clone();
        let id = HSTRING::from(product);
        self.app
            .run_on_main_thread(move || {
                let _ = started.send(asked.RequestPurchaseAsync(&id));
            })
            .map_err(|e| e.to_string())?;
        let purchase = purchase
            .recv()
            .map_err(|_| "the purchase was never started".to_string())?
            .map_err(|e| e.to_string())?;
        let result = purchase.get().map_err(|e| e.to_string())?;
        let status = result.Status().map_err(|e| e.to_string())?;
        match status {
            StorePurchaseStatus::Succeeded | StorePurchaseStatus::AlreadyPurchased => {}
            StorePurchaseStatus::NotPurchased => return Ok(()),
            other => {
                let error = result.ExtendedError().map(|e| e.message()).unwrap_or_default();
                return Err(format!("the Store said {} ({error})", other.0));
            }
        }

        let Some(Ticket { ticket, user }) = self.ticket()? else {
            eprintln!("{product:?} is bought, but the page gave no ticket to prove it with");
            return Ok(());
        };
        let key = context
            .GetCustomerCollectionsIdAsync(&HSTRING::from(ticket), &HSTRING::from(user))
            .and_then(|asked| asked.get())
            .map_err(|e| format!("no key for the purchase: {e}"))?
            .to_string();
        if key.is_empty() {
            return Err("the Store gave an empty key".to_string());
        }
        self.eval(store::purchased_script(&[&key]));
        Ok(())
    }

    /// Asks the page for a ticket (`super::ticket_script`) and waits for
    /// its answer.
    fn ticket(&self) -> Result<Option<Ticket>, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = mpsc::channel();
        self.waiting.lock().unwrap().insert(id, reply);
        self.eval(ticket_script(id));
        let answer = answer.recv_timeout(TICKET_TIMEOUT);
        self.waiting.lock().unwrap().remove(&id);
        answer.map_err(|_| "the page did not answer for a ticket".to_string())
    }
}
