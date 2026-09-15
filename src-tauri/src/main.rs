//! main entry point — delegates to `mergedrag_lib::run()`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mergedrag_lib::run()
}
