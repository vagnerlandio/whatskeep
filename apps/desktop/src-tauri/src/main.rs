// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The binary crate is named `WhatsKeep` (the executable name the Dock shows).
#![allow(non_snake_case)]

fn main() {
    whatskeep_lib::run()
}
