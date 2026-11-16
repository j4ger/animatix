//! Public engine contracts share process-wide font initialization.
//!
//! Each module keeps its original regression cases. A previous command such as
//! `cargo test --test ir_tests case_name` is now
//! `cargo test --test engine_contracts ir_tests::case_name`.

mod applicability_table_agrees_with_reads;
mod captions_and_speech_sync;
mod full_pipeline_contract;
mod geometry_count_contract;
mod ir_opt_tests;
mod ir_tests;
mod layout_invalidation_contract;
mod module_tests;
mod painter_order_annotations;
mod plot_params_tests;
mod semantic_regression;
mod svg_import_tests;
