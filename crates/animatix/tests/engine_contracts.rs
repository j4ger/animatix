//! Public engine contracts share process-wide font initialization.
//!
//! Each module keeps its original regression cases. A previous command such as
//! `cargo test --test ir_tests case_name` is now
//! `cargo test --test engine_contracts ir_tests::case_name`.

mod applicability_table_agrees_with_reads;
mod captions_and_speech_sync;
mod ir_opt_tests;
mod ir_tests;
mod module_tests;
mod plot_params_tests;
mod svg_import_tests;
