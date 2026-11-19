// Protection against prototype pollution (what Tauri's `freezePrototype`
// does), but only in the main window. Tauri's global `freezePrototype` applies
// to ALL windows, including the Google login one, and breaks Google's
// JavaScript ("Attempted to assign to readonly property"): the login's "Next"
// ("Avançar") button stops working. This module is the first import of
// main.tsx, so it runs before any other app code.
Object.freeze(Object.prototype);

export {};
