use anyhow::{Context, Result};
use itertools::Itertools;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    hash::Hash,
    ops::Range,
    sync::{Arc, RwLock},
};
use walrus::{
    ConstExpr, DataKind, ElementItems, ElementKind, ExportId, ExportItem, FunctionBuilder,
    FunctionId, FunctionKind, GlobalKind, ImportId, ImportKind, Module, ModuleConfig, RefType,
    TableId, TypeId,
    ir::{self, Visitor, dfs_in_order},
};
use wasmparser::{
    BinaryReader, Linking, LinkingSectionReader, Payload, RelocSectionReader, RelocationEntry,
    SymbolInfo,
};

pub const MAKE_LOAD_JS: &str = include_str!("./__wasm_split.js");

/// A parsed wasm module with additional metadata and functionality for splitting and patching.
///
/// This struct assumes that relocations will be present in incoming wasm binary.
/// Upon construction, all the required metadata will be constructed.
pub struct Splitter<'a> {
    /// The original module we use as a reference
    source_module: Module,

    // The byte sources of the pre and post wasm-bindgen .wasm files
    // We need the original around since wasm-bindgen ruins the relocation locations.
    original: &'a [u8],
    bindgened: &'a [u8],

    // Mapping of indices of source functions
    // This lets us use a much faster approach to emitting split modules simply by maintaining a mapping
    // between the original Module and the new Module. Ideally we could just index the new module
    // with old FunctionIds but the underlying IndexMap actually checks that a key belongs to a particular
    // arena.
    fns_to_ids: HashMap<FunctionId, usize>,
    _ids_to_fns: Vec<FunctionId>,

    shared_symbols: BTreeSet<Node>,
    split_points: Vec<SplitPoint>,
    chunks: Vec<HashSet<Node>>,
    data_symbols: BTreeMap<usize, DataSymbol>,
    main_graph: HashSet<Node>,
    call_graph: HashMap<Node, HashSet<Node>>,
    parent_graph: HashMap<Node, HashSet<Node>>,
}

/// The results of splitting the wasm module with some additional metadata for later use.
pub struct OutputModules {
    /// The main chunk
    pub main: SplitModule,

    /// The modules of the wasm module that were split.
    pub modules: Vec<SplitModule>,

    /// The chunks that might be imported by the main modules
    pub chunks: Vec<SplitModule>,
}

/// A wasm module that was split from the main module.
///
/// All IDs here correspond to *this* module - not the parent main module
pub struct SplitModule {
    pub module_name: String,
    pub hash_id: Option<String>,
    pub component_name: Option<String>,
    pub bytes: Vec<u8>,
    pub relies_on_chunks: HashSet<usize>,
}

impl<'a> Splitter<'a> {
    /// Create a new "splitter" instance using the original wasm and the wasm from the output of wasm-bindgen.
    ///
    /// This will use the relocation data from the original module to create a call graph that we
    /// then use with the post-bindgened module to create the split modules.
    ///
    /// It's important to compile the wasm with --emit-relocs such that the relocations are available
    /// to construct the callgraph.
    pub fn new(original: &'a [u8], bindgened: &'a [u8]) -> Result<Self> {
        let (module, ids, fns_to_ids) = parse_module_with_ids(bindgened)?;
        validate_supported_abi(&module)?;

        let split_points = accumulate_split_points(&module);

        // Note that we can't trust the normal symbols - just the data symbols - and we can't use the data offset
        // since that's not reliable after bindgening
        let raw_data = parse_bytes_to_data_segment(bindgened)?;

        let mut module = Self {
            source_module: module,
            original,
            bindgened,
            split_points,
            data_symbols: raw_data.data_symbols,
            _ids_to_fns: ids,
            fns_to_ids,
            main_graph: Default::default(),
            chunks: Default::default(),
            call_graph: Default::default(),
            parent_graph: Default::default(),
            shared_symbols: Default::default(),
        };

        module.build_call_graph()?;
        module.build_split_chunks();

        Ok(module)
    }

    /// Split the module into multiple modules at the boundaries of split points.
    ///
    /// Note that the binaries might still be "large" at the end of this process. In practice, you
    /// need to push these binaries through wasm-bindgen and wasm-opt to take advantage of the
    /// optimizations and splitting. We perform a few steps like zero-ing out the data segments
    /// that will only be removed by the memory-packing step of wasm-opt.
    ///
    /// This returns the list of chunks, an import map, and some javascript to link everything together.
    pub fn emit(self) -> Result<OutputModules> {
        tracing::info!("Emitting split modules.");

        let chunks = (0..self.chunks.len())
            .into_par_iter()
            .map(|idx| self.emit_split_chunk(idx))
            .collect::<Result<Vec<SplitModule>>>()?;

        let modules = (0..self.split_points.len())
            .into_par_iter()
            .map(|idx| self.emit_split_module(idx))
            .collect::<Result<Vec<SplitModule>>>()?;

        // Emit the main module, consuming self since we're going to
        let main = self.emit_main_module()?;

        Ok(OutputModules {
            modules,
            chunks,
            main,
        })
    }

    /// Emit the main module.
    ///
    /// This will analyze the call graph and then perform some transformations on the module.
    /// - Clear out active segments that the split modules will initialize
    /// - Wipe away unused functions and data symbols
    /// - Re-export the memories, globals, and other items that the split modules will need
    /// - Convert the split module import functions to real functions that call the indirect function
    ///
    /// Once this is done, all the split module functions will have been removed, making the main module smaller.
    ///
    /// Emitting the main module is conceptually pretty simple. Emitting the split modules is more
    /// complex.
    fn emit_main_module(mut self) -> Result<SplitModule> {
        tracing::info!("Emitting main bundle split module");

        // Perform some analysis of the module before we start messing with it
        let unused_symbols = self.unused_main_symbols();

        // Use the original module that contains all the right ids
        let mut out = std::mem::take(&mut self.source_module);

        // 1. Clear out the active segments that try to initialize functions for modules we just split off.
        //    When the side modules load, they will initialize functions into the table where the "holes" are.
        self.replace_segments_with_holes(&mut out, &unused_symbols);


        // 2. Wipe away the unused functions and data symbols
        self.prune_main_symbols(&mut out, &unused_symbols)?;

        // 3. Change the functions called from split modules to be local functions that call the indirect function
        self.create_ifunc_table(&mut out);

        // 4. Re-export the memories, globals, and other stuff
        self.re_export_items(&mut out);

        // 6. Remove the reloc and linking custom sections
        self.remove_custom_sections(&mut out);

        // 7. Run the garbage collector to remove unused functions
        walrus::passes::gc::run(&mut out);

        Ok(SplitModule {
            module_name: "main".to_string(),
            component_name: None,
            bytes: out.emit_wasm(),
            relies_on_chunks: Default::default(),
            hash_id: None,
        })
    }

    /// Write the contents of the split modules to the output
    fn emit_split_module(&self, split_idx: usize) -> Result<SplitModule> {
        let split = self.split_points[split_idx].clone();

        // These are the symbols that will only exist in this module and not in the main module.
        let mut unique_symbols = split
            .reachable_graph
            .difference(&self.main_graph)
            .cloned()
            .collect::<HashSet<_>>();

        // The functions we'll need to import
        let mut symbols_to_import: HashSet<_> = split
            .reachable_graph
            .intersection(&self.main_graph)
            .cloned()
            .collect();

        // Identify the functions we'll delete
        let symbols_to_delete: HashSet<_> = self
            .main_graph
            .difference(&split.reachable_graph)
            .cloned()
            .collect();

        // Convert split chunk functions to imports
        let mut relies_on_chunks = HashSet::new();
        for (idx, chunk) in self.chunks.iter().enumerate() {
            let nodes_to_extract = unique_symbols
                .intersection(chunk)
                .cloned()
                .collect::<Vec<_>>();
            for node in nodes_to_extract {
                if !self.main_graph.contains(&node) {
                    unique_symbols.remove(&node);
                    symbols_to_import.insert(node);
                    relies_on_chunks.insert(idx);
                }
            }
        }

        tracing::info!(
            "Emitting module {}/{} {}: {:?}",
            split_idx,
            self.split_points.len(),
            split.module_name,
            relies_on_chunks
        );

        let (mut out, ids_to_fns, _fns_to_ids) = parse_module_with_ids(self.bindgened)?;

        // Remap the graph to our module's IDs
        let shared_funcs = self
            .shared_symbols
            .iter()
            .filter(|node| matches!(node, Node::Function(_)))
            .map(|f| self.remap_id(&ids_to_fns, f))
            .collect::<Result<Vec<_>>>()?;

        let unique_symbols = self.remap_ids(&unique_symbols, &ids_to_fns)?;
        let symbols_to_delete = self.remap_ids(&symbols_to_delete, &ids_to_fns)?;
        let symbols_to_import = self.remap_ids(&symbols_to_import, &ids_to_fns)?;
        let split_export_func = self.remap_function(&ids_to_fns, split.export_func)?;

        // Do some basic cleanup of the module to make it smaller
        // This removes exports, imports, and the start function
        self.prune_split_module(&mut out);

        // Clear away the data segments
        self.clear_data_segments(&mut out, &unique_symbols)?;

        // Clear out the element segments and then add in the initializers for the shared imports
        self.create_ifunc_initializers(&mut out, &unique_symbols);

        // Convert our split module's functions to real functions that call the indirect function
        self.add_split_imports(
            &mut out,
            split.index,
            split_export_func,
            split.export_name,
            &symbols_to_import,
            &shared_funcs,
        );

        // Delete all the functions that are not reachable from the main module
        self.delete_main_funcs_from_split(&mut out, &symbols_to_delete);

        // Remove the reloc and linking custom sections
        self.remove_custom_sections(&mut out);

        // Run the gc to remove unused functions - also validates the module to ensure we can emit it properly
        // todo(jon): prefer to delete the items as we go so we don't need to run a gc pass. it/it's quite slow
        walrus::passes::gc::run(&mut out);

        Ok(SplitModule {
            bytes: out.emit_wasm(),
            module_name: split.module_name.clone(),
            component_name: Some(split.component_name.clone()),
            relies_on_chunks,
            hash_id: Some(split.hash_name.clone()),
        })
    }

    /// Write a split chunk - this is a chunk with no special functions, just exports + initializers
    fn emit_split_chunk(&self, idx: usize) -> Result<SplitModule> {
        tracing::info!("emitting chunk {}", idx);

        let unique_symbols = &self.chunks[idx];

        // The functions we'll need to import
        let chunk_reachable = reachable_graph(&self.call_graph, unique_symbols);

        let symbols_to_import: HashSet<_> = chunk_reachable
            .intersection(&self.main_graph)
            .cloned()
            .collect();

        // Delete everything except the symbols that are reachable from this module
        let symbols_to_delete: HashSet<_> = self
            .main_graph
            .difference(&chunk_reachable)
            .cloned()
            .collect();

        // Make sure to remap any ids from the main module to this module
        let (mut out, ids_to_fns, _fns_to_ids) = parse_module_with_ids(self.bindgened)?;

        // Remap the graph to our module's IDs
        let shared_funcs = self
            .shared_symbols
            .iter()
            .filter(|node| matches!(node, Node::Function(_)))
            .map(|f| self.remap_id(&ids_to_fns, f))
            .collect::<Result<Vec<_>>>()?;

        let unique_symbols = self.remap_ids(unique_symbols, &ids_to_fns)?;
        let symbols_to_import = self.remap_ids(&symbols_to_import, &ids_to_fns)?;
        let symbols_to_delete = self.remap_ids(&symbols_to_delete, &ids_to_fns)?;

        self.prune_split_module(&mut out);

        // Clear away the data segments
        self.clear_data_segments(&mut out, &unique_symbols)?;

        // Clear out the element segments and then add in the initializers for the shared imports
        self.create_ifunc_initializers(&mut out, &unique_symbols);

        // We have to make sure our table matches that of the other tables even though we don't call them.
        let ifunc_table_id = self.load_funcref_table(&mut out);
        let segment_start = self
            .expand_ifunc_table_max(
                &mut out,
                ifunc_table_id,
                self.split_points.len() + shared_funcs.len(),
            )
            .unwrap();

        self.convert_shared_to_imports(&mut out, segment_start, &shared_funcs, &symbols_to_import);

        // Make sure we haven't deleted anything important....
        self.delete_main_funcs_from_split(&mut out, &symbols_to_delete);

        // Remove the reloc and linking custom sections
        self.remove_custom_sections(&mut out);

        // Run the gc to remove unused functions - also validates the module to ensure we can emit it properly
        walrus::passes::gc::run(&mut out);

        Ok(SplitModule {
            bytes: out.emit_wasm(),
            module_name: "split".to_string(),
            component_name: None,
            relies_on_chunks: Default::default(),
            hash_id: None,
        })
    }

    /// Convert functions coming in from outside the module to indirect calls to the ifunc table created in the main module
    fn convert_shared_to_imports(
        &self,
        out: &mut Module,
        segment_start: usize,
        ifuncs: &Vec<Node>,
        symbols_to_import: &HashSet<Node>,
    ) {
        let ifunc_table_id = self.load_funcref_table(out);

        let mut idx = self.split_points.len();
        for node in ifuncs {
            if let Node::Function(ifunc) = node {
                if symbols_to_import.contains(node) {
                    let ty_id = out.funcs.get(*ifunc).ty();
                    let stub = (idx + segment_start) as _;
                    out.funcs.get_mut(*ifunc).kind =
                        self.make_stub_funcs(out, ifunc_table_id, ty_id, stub);
                }

                idx += 1;
            }
        }
    }

    /// Convert split import functions to local functions that call an indirect function that will
    /// be filled in from the loaded split module.
    ///
    /// This is because these imports are going to be delayed until the split module is loaded
    /// and loading in the main module these as imports won't be possible since the imports won't
    /// be resolved until the split module is loaded.
    fn create_ifunc_table(&self, out: &mut Module) {
        let ifunc_table = self.load_funcref_table(out);
        let dummy_func = self.make_dummy_func(out);

        out.exports.add("__indirect_function_table", ifunc_table);

        // Expand the ifunc table to accommodate the new ifuncs
        let segment_start = self
            .expand_ifunc_table_max(
                out,
                ifunc_table,
                self.split_points.len()
                    + self.shared_symbols.iter().filter(|node| matches!(node, Node::Function(_))).count(),
            )
            .expect("failed to expand ifunc table");

        // Delete the split import functions and replace them with local functions
        //
        // Start by pushing all the shared imports into the list
        // These don't require an additional stub function
        let mut ifuncs = vec![];

        // Push the split import functions into the list - after we've pushed in the shared imports
        for idx in 0..self.split_points.len() {
            // this is okay since we're in the main module
            let import_func = self.split_points[idx].import_func;
            let import_id = self.split_points[idx].import_id;
            let ty_id = out.funcs.get(import_func).ty();
            let stub_idx = segment_start + ifuncs.len();

            // Replace the import function with a local function that calls the indirect function
            out.funcs.get_mut(import_func).kind =
                self.make_stub_funcs(out, ifunc_table, ty_id, stub_idx as _);

            // And remove the corresponding import
            out.imports.delete(import_id);

            // Push into the list the properly typed dummy func so the entry is populated
            // unclear if the typing is important here
            ifuncs.push(dummy_func);
        }

        // Add the stub functions to the ifunc table
        // The callers of these functions will call the stub instead of the import
        let mut _idx = 0;
        for func in self.shared_symbols.iter() {
            if let Node::Function(id) = func {
                ifuncs.push(*id);
                _idx += 1;
            }
        }

        // Now add segments to the ifunc table
        out.tables
            .get_mut(ifunc_table)
            .elem_segments
            .insert(out.elements.add(
                ElementKind::Active {
                    table: ifunc_table,
                    offset: ConstExpr::Value(ir::Value::I32(segment_start as _)),
                },
                ElementItems::Functions(ifuncs),
            ));
    }

    /// Re-export the memories, globals, and other items from the main module to the side modules
    fn re_export_items(&self, out: &mut Module) {
        // Re-export memories
        for (idx, memory) in out.memories.iter().enumerate() {
            let name = memory
                .name
                .clone()
                .unwrap_or_else(|| format!("__memory_{}", idx));
            out.exports.add(&name, memory.id());
        }

        // Re-export globals
        for (idx, global) in out.globals.iter().enumerate() {
            let global_name = format!("__global__{idx}");
            out.exports.add(&global_name, global.id());
        }

        // Export any tables
        for (idx, table) in out.tables.iter().enumerate() {
            if table.element_ty != RefType::FUNCREF {
                let table_name = format!("__imported_table_{}", idx);
                out.exports.add(&table_name, table.id());
            }
        }
    }

    fn prune_main_symbols(&self, out: &mut Module, unused_symbols: &HashSet<Node>) -> Result<()> {
        // Wipe the split point exports
        for split in self.split_points.iter() {
            // it's okay that we're not re-mapping IDs since this is just used by the main module
            out.exports.delete(split.export_id);
        }

        // Preserve the byte union of every main-owned symbol. Linkers may emit
        // overlapping aliases (including aliases with different sizes), so an
        // unused alias must never zero bytes owned by a live symbol.
        let main_owned_ranges = self.main_owned_data_ranges();

        // And then any actual symbols from the callgraph
        for symbol in unused_symbols.iter().cloned() {
            match symbol {
                // Simply delete functions
                Node::Function(id) => {
                    out.funcs.delete(id);
                }

                // Otherwise, zero out the data segment, which should lead to elimination by wasm-opt
                Node::DataSymbol(id) => {
                    let symbol = self
                        .data_symbols
                        .get(&id)
                        .context("Failed to find data symbol")?;

                    // VERY IMPORTANT
                    //
                    // apparently wasm-bindgen makes data segments that aren't the main one
                    // even *touching* those will break the vtable / binding layer
                    // We can only interact with the first data segment - the rest need to stay available
                    // for the `.js` to interact with.
                    if symbol.which_data_segment == 0 {
                        let data_id = out.data.iter().nth(symbol.which_data_segment).unwrap().id();
                        let data = out.data.get_mut(data_id);
                        let symbol_range = symbol.segment_offset..symbol.segment_offset + symbol.symbol_size;
                        for range in subtract_ranges(symbol_range, &main_owned_ranges) {
                            data.value[range].fill(0);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // 2.1 Create a dummy func that will be overridden later as modules pop in
    // 2.2 swap the segment entries with the dummy func, leaving hole in its placed that will be filled in later
    fn replace_segments_with_holes(
        &self,
        out: &mut Module,
        unused_symbols: &HashSet<Node>,
    ) -> FunctionId {
        let dummy_func = self.make_dummy_func(out);
        for element in out.elements.iter_mut() {
            match &mut element.items {
                ElementItems::Functions(vec) => {
                    for item in vec.iter_mut() {
                        if unused_symbols.contains(&Node::Function(*item)) {
                            *item = dummy_func;
                        }
                    }
                }
                ElementItems::Expressions(_ref_type, const_exprs) => {
                    for item in const_exprs.iter_mut() {
                        if let &mut ConstExpr::RefFunc(id) = item {
                            if unused_symbols.contains(&Node::Function(id)) {
                                *item = ConstExpr::RefFunc(dummy_func);
                            }
                        }
                    }
                }
            }
        }
        dummy_func
    }

    /// Creates the jump points
    fn create_ifunc_initializers(&self, out: &mut Module, unique_symbols: &HashSet<Node>) {
        let ifunc_table = self.load_funcref_table(out);

        let mut initializers = HashMap::new();
        for segment in out.elements.iter_mut() {
            let ElementKind::Active { offset, .. } = &mut segment.kind else {
                continue;
            };

            let ConstExpr::Value(ir::Value::I32(offset)) = offset else {
                continue;
            };

            match &segment.items {
                ElementItems::Functions(vec) => {
                    for (idx, id) in vec.iter().enumerate() {
                        if unique_symbols.contains(&Node::Function(*id)) {
                            initializers
                                .insert(*offset + idx as i32, ElementItems::Functions(vec![*id]));
                        }
                    }
                }

                ElementItems::Expressions(ref_type, const_exprs) => {
                    for (idx, expr) in const_exprs.iter().enumerate() {
                        if let ConstExpr::RefFunc(id) = expr {
                            if unique_symbols.contains(&Node::Function(*id)) {
                                initializers.insert(
                                    *offset + idx as i32,
                                    ElementItems::Expressions(
                                        *ref_type,
                                        vec![ConstExpr::RefFunc(*id)],
                                    ),
                                );
                            }
                        }
                    }
                }
            }
        }

        // Wipe away references to these segments
        for table in out.tables.iter_mut() {
            table.elem_segments.clear();
        }

        // Wipe away the element segments themselves
        let segments_to_delete: Vec<_> = out.elements.iter().map(|e| e.id()).collect();
        for id in segments_to_delete {
            out.elements.delete(id);
        }

        // Add in our new segments
        let ifunc_table_ = out.tables.get_mut(ifunc_table);
        for (offset, items) in initializers {
            let kind = ElementKind::Active {
                table: ifunc_table,
                offset: ConstExpr::Value(ir::Value::I32(offset)),
            };

            ifunc_table_
                .elem_segments
                .insert(out.elements.add(kind, items));
        }
    }

    fn add_split_imports(
        &self,
        out: &mut Module,
        split_idx: usize,
        split_export_func: FunctionId,
        split_export_name: String,
        symbols_to_import: &HashSet<Node>,
        ifuncs: &Vec<Node>,
    ) {
        let ifunc_table_id = self.load_funcref_table(out);
        let segment_start = self
            .expand_ifunc_table_max(out, ifunc_table_id, self.split_points.len() + ifuncs.len())
            .unwrap();

        // Make sure to re-export the split func
        out.exports.add(&split_export_name, split_export_func);

        // Add the elements back to the table
        out.tables
            .get_mut(ifunc_table_id)
            .elem_segments
            .insert(out.elements.add(
                ElementKind::Active {
                    table: ifunc_table_id,
                    offset: ConstExpr::Value(ir::Value::I32((segment_start + split_idx) as i32)),
                },
                ElementItems::Functions(vec![split_export_func]),
            ));

        self.convert_shared_to_imports(out, segment_start, ifuncs, symbols_to_import);
    }

    fn delete_main_funcs_from_split(&self, out: &mut Module, symbols_to_delete: &HashSet<Node>) {
        for node in symbols_to_delete {
            if let Node::Function(id) = *node {
                // if out.exports.get_exported_func(id).is_none() {
                out.funcs.delete(id);
                // }
            }
        }
    }

    /// Remove un-needed stuff and then hoist
    fn prune_split_module(&self, out: &mut Module) {
        // Clear the module's start/main
        if let Some(start) = out.start.take() {
            if let Some(export) = out.exports.get_exported_func(start) {
                out.exports.delete(export.id());
            }
        }

        // We're going to import the funcref table, so wipe it altogether
        for table in out.tables.iter_mut() {
            table.elem_segments.clear();
        }

        // Wipe all our imports - we're going to use a different set of imports
        let all_imports: HashSet<_> = out.imports.iter().map(|i| i.id()).collect();
        for import_id in all_imports {
            out.imports.delete(import_id);
        }

        // Wipe away memories
        let all_memories: Vec<_> = out.memories.iter().map(|m| m.id()).collect();
        for memory_id in all_memories {
            out.memories.get_mut(memory_id).data_segments.clear();
        }

        // Add exports that call the corresponding import
        let exports = out.exports.iter().map(|e| e.id()).collect::<Vec<_>>();
        for export_id in exports {
            out.exports.delete(export_id);
        }

        // Convert the tables to imports.
        // Should be as simple as adding a new import and then writing the `.import` field
        for (idx, table) in out.tables.iter_mut().enumerate() {
            let name = table.name.clone().unwrap_or_else(|| {
                if table.element_ty == RefType::FUNCREF {
                    "__indirect_function_table".to_string()
                } else {
                    format!("__imported_table_{}", idx)
                }
            });
            let import = out.imports.add("__wasm_split", &name, table.id());
            table.import = Some(import);
        }

        // Convert the memories to imports
        // Should be as simple as adding a new import and then writing the `.import` field
        for (idx, memory) in out.memories.iter_mut().enumerate() {
            let name = memory
                .name
                .clone()
                .unwrap_or_else(|| format!("__memory_{}", idx));
            let import = out.imports.add("__wasm_split", &name, memory.id());
            memory.import = Some(import);
        }

        // Convert the globals to imports
        // We might not use the global, so if we don't, we can just get
        let global_ids: Vec<_> = out.globals.iter().map(|t| t.id()).collect();
        for (idx, global_id) in global_ids.into_iter().enumerate() {
            let global = out.globals.get_mut(global_id);
            let global_name = format!("__global__{idx}");
            let import = out.imports.add("__wasm_split", &global_name, global.id());
            global.kind = GlobalKind::Import(import);
        }
    }

    fn make_dummy_func(&self, out: &mut Module) -> FunctionId {
        let mut b = FunctionBuilder::new(&mut out.types, &[], &[]);
        b.name("dummy".into()).func_body().unreachable();
        b.finish(vec![], &mut out.funcs)
    }

    fn clear_data_segments(&self, out: &mut Module, unique_symbols: &HashSet<Node>) -> Result<()> {
        // Validate every segment and every symbol range before mutating any bytes. Split modules
        // only copy owned symbols from the primary active segment; secondary segments are owned
        // by main and are deliberately emptied from the side module.
        for (idx, data) in out.data.iter().enumerate() {
            let DataKind::Active { offset, .. } = &data.kind else {
                anyhow::bail!("unsupported non-active data segment {idx} while splitting");
            };
            if idx == 0 && !matches!(offset, ConstExpr::Value(ir::Value::I32(_))) {
                anyhow::bail!("unsupported non-constant-i32 offset in primary data segment");
            }
        }
        for node in unique_symbols {
            let Node::DataSymbol(id) = node else { continue };
            let symbol = self.data_symbols.get(id)
                .with_context(|| format!("missing data symbol metadata for {id}"))?;
            let data = out.data.iter().nth(symbol.which_data_segment)
                .with_context(|| format!("data symbol {id} refers to missing segment {}", symbol.which_data_segment))?;
            checked_data_symbol_range(data.value.len(), symbol.segment_offset, symbol.symbol_size)
                .with_context(|| format!("invalid data range for symbol {id} in segment {}", symbol.which_data_segment))?;
        }

        // Preserve the data symbols for this module and then clear them away
        let main_owned_ranges = self.main_owned_data_ranges();
        let data_ids: Vec<_> = out.data.iter().map(|t| t.id()).collect();
        for (idx, data_id) in data_ids.into_iter().enumerate() {
            let data = out.data.get_mut(data_id);

            // Take the data out of the vec - zeroing it out unless we patch it in manually
            let contents = data.value.split_off(0);

            // Zero out the non-primary data segments
            if idx != 0 {
                continue;
            }

            let DataKind::Active { memory, offset } = &data.kind else {
                continue;
            };
            let memory = *memory;

            let ConstExpr::Value(ir::Value::I32(data_offset)) = *offset else {
                continue;
            };

            // And then assign chunks of the data to new data entries that will override the individual slots
            for unique in unique_symbols {
                if let Node::DataSymbol(id) = unique {
                    if let Some(symbol) = self.data_symbols.get(id) {
                        if symbol.which_data_segment == idx {
                            // Zero-sized definitions retain graph/address identity but own no
                            // bytes to copy into a side module.
                            if symbol.symbol_size == 0 {
                                continue;
                            }
                            let range = checked_data_symbol_range(
                                contents.len(),
                                symbol.segment_offset,
                                symbol.symbol_size,
                            )?;
                            for owned_range in subtract_ranges(range, &main_owned_ranges) {
                                let offset = ConstExpr::Value(ir::Value::I32(
                                    data_offset + owned_range.start as i32,
                                ));
                                out.data.add(
                                    DataKind::Active { memory, offset },
                                    contents[owned_range].to_vec(),
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn data_symbol_ranges(&self, nodes: &HashSet<Node>) -> Vec<Range<usize>> {
        let mut ranges = nodes
            .iter()
            .filter_map(|node| match node {
                Node::DataSymbol(id) => self.data_symbols.get(id),
                Node::Function(_) => None,
            })
            .filter(|symbol| symbol.which_data_segment == 0 && symbol.symbol_size > 0)
            .map(|symbol| symbol.segment_offset..symbol.segment_offset + symbol.symbol_size)
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| (range.start, range.end));
        merge_ranges(ranges)
    }

    fn main_owned_data_ranges(&self) -> Vec<Range<usize>> {
        let split_data: HashSet<Node> = self.split_points.iter()
            .flat_map(|split| split.reachable_graph.iter())
            .filter(|node| matches!(node, Node::DataSymbol(_)))
            .copied()
            .collect();
        let main_data: HashSet<Node> = self.main_graph.iter()
            .filter(|node| matches!(node, Node::DataSymbol(_)))
            .copied()
            .collect();
        let owned = self.data_symbols.keys().filter_map(|id| {
            let node = Node::DataSymbol(*id);
            (main_data.contains(&node) || !split_data.contains(&node)).then_some(node)
        }).collect();
        self.data_symbol_ranges(&owned)
    }

    /// Load the funcref table from the main module. This *should* exist for all modules created by
    /// Rustc or Wasm-Bindgen, but we create it if it doesn't exist.
    fn load_funcref_table(&self, out: &mut Module) -> TableId {
        let ifunc_table = out
            .tables
            .iter()
            .find(|t| t.element_ty == RefType::FUNCREF)
            .map(|t| t.id());

        if let Some(table) = ifunc_table {
            table
        } else {
            out.tables.add_local(false, 0, None, RefType::FUNCREF)
        }
    }

    /// Convert the imported function to a local function that calls an indirect function from the table
    ///
    /// This will enable the main module (and split modules) to call functions from outside their own module.
    /// The functions might not exist when the main module is loaded, so we'll register some elements
    /// that fill those in eventually.
    fn make_stub_funcs(
        &self,
        out: &mut Module,
        table: TableId,
        ty_id: TypeId,
        table_idx: i32,
    ) -> FunctionKind {
        // Convert the import function to a local function that calls the indirect function from the table
        let ty = out.types.get(ty_id);

        let params = ty.params().to_vec();
        let results = ty.results().to_vec();
        let args: Vec<_> = params.iter().map(|ty| out.locals.add(*ty)).collect();

        // New function that calls the indirect function
        let mut builder = FunctionBuilder::new(&mut out.types, &params, &results);
        let mut body = builder.name("stub".into()).func_body();

        // Push the params onto the stack
        for arg in args.iter() {
            body.local_get(*arg);
        }

        // And then the address of the indirect function
        body.instr(ir::Instr::Const(ir::Const {
            value: ir::Value::I32(table_idx),
        }));

        // And call it
        body.instr(ir::Instr::CallIndirect(ir::CallIndirect {
            ty: ty_id,
            table,
        }));

        FunctionKind::Local(builder.local_func(args))
    }

    /// Expand the ifunc table to accommodate the new ifuncs
    ///
    /// returns the old maximum
    fn expand_ifunc_table_max(
        &self,
        out: &mut Module,
        table: TableId,
        num_ifuncs: usize,
    ) -> Option<usize> {
        let ifunc_table_ = out.tables.get_mut(table);

        if let Some(max) = ifunc_table_.maximum {
            ifunc_table_.maximum = Some(max + num_ifuncs as u64);
            ifunc_table_.initial += num_ifuncs as u64;
            return Some(max as usize);
        }

        None
    }

    // only keep the target-features and names section so wasm-opt can use it to optimize the output
    fn remove_custom_sections(&self, out: &mut Module) {
        let sections_to_delete = out
            .customs
            .iter()
            .filter_map(|(id, section)| {
                if section.name() == "target_features" {
                    None
                } else {
                    Some(id)
                }
            })
            .collect::<Vec<_>>();

        for id in sections_to_delete {
            out.customs.delete(id);
        }
    }

    /// Accumulate any shared funcs between multiple chunks into a single residual chunk.
    /// This prevents duplicates from being downloaded.
    /// Eventually we need to group the chunks into smarter "communities" - ie the Louvain algorithm
    ///
    /// Todo: we could chunk up the main module itself! Not going to now but it would enable parallel downloads of the main chunk
    fn build_split_chunks(&mut self) {
        // create a single chunk that contains all functions used by multiple modules
        let mut funcs_used_by_chunks: HashMap<Node, HashSet<usize>> = HashMap::new();
        for (split_idx, split) in self.split_points.iter().enumerate() {
            for item in split.reachable_graph.iter() {
                if self.main_graph.contains(item) {
                    continue;
                }
                funcs_used_by_chunks
                    .entry(*item)
                    .or_default()
                    .insert(split_idx);
            }
        }

        // Only consider funcs that are used by multiple modules - otherwise they can just stay in their respective module
        funcs_used_by_chunks.retain(|_, v| v.len() > 1);

        // todo: break down this chunk if it exceeds a certain size (100kb?) by identifying different groups

        self.chunks
            .push(funcs_used_by_chunks.keys().cloned().collect());
    }

    fn unused_main_symbols(&self) -> HashSet<Node> {
        self.split_points
            .iter()
            .flat_map(|split| split.reachable_graph.iter())
            .filter(|sym| {
                // Make sure the symbol isn't in the main graph
                if self.main_graph.contains(sym) {
                    return false;
                }

                // And ensure we aren't also exporting it
                match sym {
                    Node::Function(u) => self.source_module.exports.get_exported_func(*u).is_none(),
                    _ => true,
                }
            })
            .cloned()
            .collect()
    }

    /// Accumulate the relocations from the original module, create a relocation map, and then convert
    /// that to our *new* module's symbols.
    fn build_call_graph(&mut self) -> Result<()> {
        let original = ModuleWithRelocations::new(self.original)?;

        let old_names: HashMap<String, FunctionId> = original
            .module
            .funcs
            .iter()
            .flat_map(|f| Some((f.name.clone()?, f.id())))
            .collect();

        let new_names: HashMap<String, FunctionId> = self
            .source_module
            .funcs
            .iter()
            .flat_map(|f| Some((f.name.clone()?, f.id())))
            .collect();

        let mut old_to_new = HashMap::new();

        validate_mangled_function_mapping(old_names.keys(), new_names.keys())?;

        for (new_name, new_func) in new_names.iter() {
            if let Some(old_func) = old_names.get(new_name) {
                old_to_new.insert(*old_func, new_func);
            }
        }

        let get_old = |old: &Node| -> Option<Node> {
            match old {
                Node::Function(id) => old_to_new.get(id).map(|new_id| Node::Function(**new_id)),
                Node::DataSymbol(id) => Some(Node::DataSymbol(*id)),
            }
        };

        // Preserve relocation edges across old functions that wasm-bindgen
        // removes or renames. For each surviving parent, walk through missing
        // intermediate nodes until reaching mapped descendants, then attach
        // those descendants to the original parent. A per-parent visited set
        // makes cycles in the original call/data graph safe. In particular,
        // do not attach recovered nodes to `main`: that would turn every
        // wasm-bindgen shim reachable through a removed describe function into
        // a main root and prevent capability splitting.
        self.call_graph = bridge_relocation_graph(&original.call_graph, |node| get_old(&node));

        // Walk the functions and try to disconnect any holes manually
        // This will attempt to resolve any of the new symbols like the shim functions
        for func in self.source_module.funcs.iter() {
            struct CallGrapher<'a> {
                cur: FunctionId,
                call_graph: &'a mut HashMap<Node, HashSet<Node>>,
            }
            impl<'a> Visitor<'a> for CallGrapher<'a> {
                fn visit_function_id(&mut self, function: &walrus::FunctionId) {
                    self.call_graph
                        .entry(Node::Function(self.cur))
                        .or_default()
                        .insert(Node::Function(*function));
                }
            }
            if let FunctionKind::Local(local) = &func.kind {
                let mut call_grapher = CallGrapher {
                    cur: func.id(),
                    call_graph: &mut self.call_graph,
                };
                dfs_in_order(&mut call_grapher, local, local.entry_block());
            }
        }

        // Fill in the parent graph
        for (parnet, children) in self.call_graph.iter() {
            for child in children {
                self.parent_graph.entry(*child).or_default().insert(*parnet);
            }
        }

        // Now go fill in the reachability graph for each of the split points
        // Keep split reachability narrow while retaining mapped descendants
        // across symbols removed by wasm-bindgen.
        self.split_points.iter_mut().for_each(|split| {
            let roots: HashSet<_> = [Node::Function(split.export_func)].into();
            split.reachable_graph = reachable_graph(&self.call_graph, &roots);
        });

        // And then the reachability graph for main. Grow the main closure to
        // include functions/data reached by Wasm table and reference usage.
        self.main_graph = reachable_graph(&self.call_graph, &self.main_roots());
        let mut pass = 0usize;
        loop {
            pass += 1;
            let (mut main_only, ids_to_fns, _) = parse_module_with_ids(self.bindgened)?;
            anyhow::ensure!(ids_to_fns.len() == self._ids_to_fns.len(), "reparsed function map changed length");
            for split in &self.split_points {
                let func = main_only.exports.get_func(&split.export_name)
                    .with_context(|| format!("missing split export {:?}", split.export_name))?;
                let export = main_only.exports.get_exported_func(func)
                    .with_context(|| format!("split export {:?} has no function export", split.export_name))?;
                main_only.exports.delete(export.id());
            }
            let mut split_only = HashSet::new();
            for symbol in self.split_points.iter().flat_map(|split| split.reachable_graph.iter()) {
                if self.main_graph.contains(symbol) { continue; }
                if let Node::Function(id) = symbol {
                    let index = self.fns_to_ids.get(id)
                        .with_context(|| format!("split-only function {id:?} is missing from source index map"))?;
                    let mapped = ids_to_fns.get(*index)
                        .with_context(|| format!("split-only function index {index} is missing from reparsed map"))?;
                    split_only.insert(Node::Function(*mapped));
                }
            }
            let dummy_func = self.replace_segments_with_holes(&mut main_only, &split_only);
            let used = wasm_used::Used::new(&main_only, &HashSet::new());
            let temp_function_indices: HashMap<_, _> = ids_to_fns.iter().enumerate()
                .map(|(index, function)| (*function, index)).collect();
            let mut newly_live = HashSet::new();
            for func in used.funcs {
                if func == dummy_func { continue; }
                let index = temp_function_indices.get(&func)
                    .with_context(|| format!("live-main function {func:?} is missing from reparsed map"))?;
                let source_func = *self._ids_to_fns.get(*index)
                    .with_context(|| format!("live-main function index {index} is absent from source map"))?;
                newly_live.insert(Node::Function(source_func));
            }
            let before = self.main_graph.len();
            self.main_graph.extend(reachable_graph(&self.call_graph, &newly_live));
            let added = self.main_graph.len() - before;
            tracing::debug!(pass, added, total = self.main_graph.len(), "expanded live main closure");
            if added == 0 { break; }
        }

        // Build the main module's actual live-function closure too. The
        // relocation graph omits some Wasm references such as `ref.func` in a
        // live global initializer. Remove split exports and replace cold
        // table entries in a clone, then let wasm-used compute precisely what
        // the remaining main exports/start still require. These targets must
        // stay in main and become shared imports for any split module using
        // them; deleting them would leave stale FunctionIds for Walrus GC.
        // Re-parse the bindgened bytes instead of cloning Walrus Module (which
        // is intentionally not Clone). Map graph IDs through symbol names so
        // this analysis does not depend on arena-ID stability across parses.
        let (mut main_only, ids_to_fns, _) = parse_module_with_ids(self.bindgened)?;
        anyhow::ensure!(
            ids_to_fns.len() == self._ids_to_fns.len(),
            "reparsed wasm function index map changed length: {} vs {}",
            ids_to_fns.len(),
            self._ids_to_fns.len()
        );
        for split in &self.split_points {
            if let Ok(func) = main_only.exports.get_func(&split.export_name) {
                if let Some(export) = main_only.exports.get_exported_func(func) {
                    main_only.exports.delete(export.id());
                }
            }
        }
        let mut split_only = HashSet::new();
        for symbol in self.split_points.iter().flat_map(|split| split.reachable_graph.iter()) {
            if self.main_graph.contains(symbol) {
                continue;
            }
            if let Node::Function(id) = symbol {
                let index = self.fns_to_ids.get(id)
                    .with_context(|| format!("split-only function {id:?} is missing from source index map"))?;
                let mapped = ids_to_fns.get(*index)
                    .with_context(|| format!("split-only function index {index} is missing from reparsed module map"))?;
                split_only.insert(Node::Function(*mapped));
            }
            // Data symbols have no Walrus function ID and are intentionally left to the
            // existing data-segment ownership path.
        }
        let dummy_func = self.replace_segments_with_holes(&mut main_only, &split_only);
        let used = wasm_used::Used::new(&main_only, &HashSet::new());
        let temp_function_indices: HashMap<_, _> = ids_to_fns
            .iter()
            .enumerate()
            .map(|(index, function)| (*function, index))
            .collect();
        for func in used.funcs {
            if func == dummy_func {
                continue;
            }
            let index = temp_function_indices.get(&func)
                .with_context(|| format!("live-main function {func:?} is missing from reparsed function index map"))?;
            anyhow::ensure!(*index < self._ids_to_fns.len(), "live-main function index {index} is absent from source function map");
            let source_func = self._ids_to_fns[*index];
            if !self.main_graph.contains(&Node::Function(source_func)) {
                tracing::debug!(
                    function = ?self.source_module.funcs.get(source_func).name,
                    function_id = ?source_func,
                    "live main function absent from relocation graph"
                );
                self.main_graph.insert(Node::Function(source_func));
            }
        }

        // And then the symbols shared between all
        self.shared_symbols = {
            let mut shared_funcs = HashSet::new();

            // Add all the symbols shared between the various modules
            for split in self.split_points.iter() {
                shared_funcs.extend(self.main_graph.intersection(&split.reachable_graph));
            }

            // And then all our imports will be callabale via the ifunc table too
            for import in self.source_module.imports.iter() {
                if let ImportKind::Function(id) = import.kind {
                    shared_funcs.insert(Node::Function(id));
                }
            }

            // Make sure to make this *ordered*
            shared_funcs.into_iter().collect()
        };

        Ok(())
    }

    fn main_roots(&self) -> HashSet<Node> {
        // Accumulate all the split entrypoints
        // This will include wasm_bindgen functions too
        let exported_splits = self
            .split_points
            .iter()
            .map(|f| f.export_func)
            .collect::<HashSet<_>>();

        // And only return the functions that are reachable from the main module's start function
        let mut roots = self
            .source_module
            .exports
            .iter()
            .filter_map(|e| match e.item {
                ExportItem::Function(id) if !exported_splits.contains(&id) => {
                    Some(Node::Function(id))
                }
                _ => None,
            })
            .chain(self.source_module.start.map(Node::Function))
            .collect::<HashSet<Node>>();

        // Also add "imports" to the roots
        for import in self.source_module.imports.iter() {
            if let ImportKind::Function(id) = import.kind {
                roots.insert(Node::Function(id));
            }
        }

        roots
    }

    /// Convert this set of nodes to reference the new module
    fn remap_ids(&self, set: &HashSet<Node>, ids_to_fns: &[FunctionId]) -> Result<HashSet<Node>> {
        let mut out = HashSet::with_capacity(set.len());
        for node in set {
            out.insert(self.remap_id(ids_to_fns, node)?);
        }
        Ok(out)
    }

    fn remap_id(&self, ids_to_fns: &[id_arena::Id<walrus::Function>], node: &Node) -> Result<Node> {
        match node {
            // Remap the function IDs
            Node::Function(id) => Ok(Node::Function(self.remap_function(ids_to_fns, *id)?)),
            // data symbols don't need remapping
            Node::DataSymbol(id) => Ok(Node::DataSymbol(*id)),
        }
    }

    fn remap_function(&self, ids_to_fns: &[FunctionId], id: FunctionId) -> Result<FunctionId> {
        let index = self.fns_to_ids.get(&id)
            .with_context(|| format!("function {id:?} is missing from source index map"))?;
        ids_to_fns.get(*index).copied()
            .with_context(|| format!("function index {index} for {id:?} is missing from reparsed module map ({} entries)", ids_to_fns.len()))
    }
}

/// Parse a module and return the mapping of index to FunctionID.
/// We'll use this mapping to remap ModuleIDs
fn parse_module_with_ids(
    bindgened: &[u8],
) -> Result<(Module, Vec<FunctionId>, HashMap<FunctionId, usize>)> {
    let ids = Arc::new(RwLock::new(Vec::new()));
    let ids_ = ids.clone();
    let module = Module::from_buffer_with_config(
        bindgened,
        ModuleConfig::new().on_parse(move |_m, our_ids| {
            let mut ids = ids_.write().expect("No shared writers");
            let mut idx = 0;
            while let Ok(entry) = our_ids.get_func(idx) {
                ids.push(entry);
                idx += 1;
            }

            Ok(())
        }),
    )?;
    let mut ids_ = ids.write().expect("No shared writers");
    let mut ids = vec![];
    std::mem::swap(&mut ids, &mut *ids_);

    let mut fns_to_ids = HashMap::new();
    for (idx, id) in ids.iter().enumerate() {
        fns_to_ids.insert(*id, idx);
    }

    Ok((module, ids, fns_to_ids))
}

struct ModuleWithRelocations<'a> {
    module: Module,
    function_ids: Vec<FunctionId>,
    symbols: Vec<SymbolInfo<'a>>,
    call_graph: HashMap<Node, HashSet<Node>>,
    parents: HashMap<Node, HashSet<Node>>,
    relocation_map: HashMap<Node, Vec<RelocationEntry>>,
    data_symbols: BTreeMap<usize, DataSymbol>,
    data_section_range: Range<usize>,
}

impl<'a> ModuleWithRelocations<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self> {
        let (module, function_ids, _) = parse_module_with_ids(bytes)?;
        let raw_data = parse_bytes_to_data_segment(bytes)?;

        let mut module = Self {
            module,
            function_ids,
            data_symbols: raw_data.data_symbols,
            data_section_range: raw_data.data_range,
            symbols: raw_data.symbols,
            call_graph: Default::default(),
            relocation_map: Default::default(),
            parents: Default::default(),
        };

        module.build_code_call_graph()?;
        module.build_data_call_graph()?;
        add_data_alias_edges(&mut module.call_graph, &module.data_symbols);

        for (func, children) in module.call_graph.iter() {
            for child in children {
                module.parents.entry(*child).or_default().insert(*func);
            }
        }

        Ok(module)
    }

    fn build_code_call_graph(&mut self) -> Result<()> {
        let codes_relocations = self.collect_relocations_from_section("reloc.CODE")?;
        let mut relocations = codes_relocations.iter().peekable();

        for (func_id, local) in self.module.funcs.iter_local() {
            let range = local
                .original_range
                .clone()
                .context("local function has no range")?;

            // Walk with relocation
            while let Some(entry) =
                relocations.next_if(|entry| entry.relocation_range().start < range.end)
            {
                let reloc_range = entry.relocation_range();
                assert!(reloc_range.start >= range.start);
                assert!(reloc_range.end <= range.end);

                if let Some(target) = self.get_symbol_dep_node(entry.index as usize)? {
                    let us = Node::Function(func_id);
                    self.call_graph.entry(us).or_default().insert(target);
                    self.relocation_map.entry(us).or_default().push(*entry);
                }
            }
        }

        assert!(relocations.next().is_none());

        Ok(())
    }

    fn build_data_call_graph(&mut self) -> Result<()> {
        let data_relocations = self.collect_relocations_from_section("reloc.DATA")?;
        let mut symbols_sorted = self
            .data_symbols
            .values()
            .map(|symbol| {
                let start = symbol.range.start - self.data_section_range.start;
                let end = symbol.range.end - self.data_section_range.start;
                (start..end, symbol.index)
            })
            .collect::<Vec<_>>();
        symbols_sorted.sort_by_key(|(range, id)| (range.start, range.end, *id));
        let mut max_ends = Vec::with_capacity(symbols_sorted.len());
        let mut max_end = 0;
        for (range, _) in &symbols_sorted {
            max_end = max_end.max(range.end);
            max_ends.push(max_end);
        }

        for entry in &data_relocations {
            let reloc_range = entry.relocation_range();
            let owners = overlapping_ranges(&symbols_sorted, &max_ends, reloc_range);
            if let Some(target) = self.get_symbol_dep_node(entry.index as usize)? {
                for owner in owners {
                    let source = Node::DataSymbol(symbols_sorted[owner].1);
                    self.call_graph.entry(source).or_default().insert(target);
                    self.relocation_map.entry(source).or_default().push(*entry);
                }
            }
        }
        Ok(())
    }

    /// Accumulate all relocations from a section.
    ///
    /// Parses the section using the RelocSectionReader and returns a vector of relocation entries.
    fn collect_relocations_from_section(&self, name: &str) -> Result<Vec<RelocationEntry>> {
        let (_reloc_id, code_reloc) = self
            .module
            .customs
            .iter()
            .find(|(_, c)| c.name() == name)
            .context("Module does not contain the reloc section")?;

        let code_reloc_data = code_reloc.data(&Default::default());
        let reader = BinaryReader::new(&code_reloc_data, 0);
        let relocations = RelocSectionReader::new(reader)
            .context("failed to parse reloc section")?
            .entries()
            .into_iter()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to parse relocation entry")?;

        Ok(relocations)
    }

    /// Get the symbol's corresponding entry in the call graph
    ///
    /// This might panic if the source module isn't built properly. Make sure to enable LTO and `--emit-relocs`
    /// when building the source module.
    fn get_symbol_dep_node(&self, index: usize) -> Result<Option<Node>> {
        let symbol = self.symbols.get(index).with_context(|| format!("relocation references missing symbol index {index}"))?;
        let res = match symbol {
            SymbolInfo::Data { .. } => Some(Node::DataSymbol(index)),
            SymbolInfo::Func { .. } => {
                let function_index = function_symbol_index(symbol, self.function_ids.len())?
                    .context("function symbol did not contain an index")?;
                Some(Node::Function(self.function_ids[function_index]))
            }

            _ => None,
        };

        Ok(res)
    }
}

#[derive(Debug, Clone)]
pub struct SplitPoint {
    module_name: String,
    import_id: ImportId,
    export_id: ExportId,
    import_func: FunctionId,
    export_func: FunctionId,
    component_name: String,
    index: usize,
    reachable_graph: HashSet<Node>,
    hash_name: String,

    #[allow(unused)]
    import_name: String,

    #[allow(unused)]
    export_name: String,
}

/// Search the module's imports and exports for functions marked as split points.
///
/// These will be in the form of:
///
/// `__wasm_split_00<module>00_<import|export>_<hash>_<function>`
///
/// For a function named `SomeRoute2` in the module `add_body_element`, the pairings would be:
///
/// `__wasm_split_00add_body_element00_import_abef5ee3ebe66ff17677c56ee392b4c2_SomeRoute2`
/// `__wasm_split_00add_body_element00_export_abef5ee3ebe66ff17677c56ee392b4c2_SomeRoute2`
///
fn accumulate_split_points(module: &Module) -> Vec<SplitPoint> {
    let mut index = 0;

    module
        .imports
        .iter()
        .sorted_by(|a, b| a.name.cmp(&b.name))
        .flat_map(|import| {
            if !import.name.starts_with("__wasm_split_00") {
                return None;
            }

            let ImportKind::Function(import_func) = import.kind else {
                return None;
            };

            // Parse the import name to get the module name, the hash, and the function name
            let remain = import.name.trim_start_matches("__wasm_split_00___");
            let (module_name, rest) = remain.split_once("___00").unwrap();
            let (hash, fn_name) = rest.trim_start_matches("_import_").split_once("_").unwrap();

            // Look for the export with the same name
            let export_name =
                format!("__wasm_split_00___{module_name}___00_export_{hash}_{fn_name}");
            let export_func = module
                .exports
                .get_func(&export_name)
                .expect("Could not find export");
            let export = module.exports.get_exported_func(export_func).unwrap();

            let our_index = index;
            index += 1;

            Some(SplitPoint {
                export_id: export.id(),
                import_id: import.id(),
                module_name: module_name.to_string(),
                import_name: import.name.clone(),
                import_func,
                export_func,
                export_name,
                hash_name: hash.to_string(),
                component_name: fn_name.to_string(),
                index: our_index,
                reachable_graph: Default::default(),
            })
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, PartialOrd, Ord, Clone)]
pub enum Node {
    Function(FunctionId),
    DataSymbol(usize),
}

/// Rebuild a graph after some source nodes have disappeared. Edges from a
/// surviving parent pass through any number of unmapped nodes and terminate at
/// the first mapped descendants. The per-parent visited set handles cycles.
fn bridge_relocation_graph<T, F>(graph: &HashMap<T, HashSet<T>>, mut map: F) -> HashMap<T, HashSet<T>>
where
    T: Copy + Eq + Hash,
    F: FnMut(T) -> Option<T>,
{
    let mut bridged = HashMap::new();
    for (old_parent, old_children) in graph {
        let Some(parent) = map(*old_parent) else {
            continue;
        };
        let mut seen = HashSet::new();
        let mut pending: Vec<T> = old_children.iter().copied().collect();
        while let Some(old_child) = pending.pop() {
            if !seen.insert(old_child) {
                continue;
            }
            if let Some(child) = map(old_child) {
                bridged.entry(parent).or_insert_with(HashSet::new).insert(child);
            } else if let Some(children) = graph.get(&old_child) {
                pending.extend(children.iter().copied());
            }
        }
    }
    bridged
}

fn reachable_graph<T>(deps: &HashMap<T, HashSet<T>>, roots: &HashSet<T>) -> HashSet<T>
where
    T: Copy + Eq + Hash,
{
    let mut queue: VecDeque<T> = roots.iter().copied().collect();
    let mut reachable = HashSet::<T>::new();

    while let Some(node) = queue.pop_front() {
        reachable.insert(node);
        let Some(children) = deps.get(&node) else {
            continue;
        };
        for child in children {
            if reachable.contains(child) {
                continue;
            }
            queue.push_back(*child);
        }
    }

    reachable
}

struct RawDataSection<'a> {
    data_range: Range<usize>,
    symbols: Vec<SymbolInfo<'a>>,
    data_symbols: BTreeMap<usize, DataSymbol>,
}

#[derive(Debug)]
struct DataSymbol {
    index: usize,
    range: Range<usize>,
    segment_offset: usize,
    symbol_size: usize,
    which_data_segment: usize,
}

/// Manually parse the data section from a wasm module
///
/// We need to do this for data symbols because walrus doesn't provide the right range and offset
/// information for data segments. Fortunately, it provides it for code sections, so we only need to
/// do a small amount extra of parsing here.
fn parse_bytes_to_data_segment(bytes: &[u8]) -> Result<RawDataSection<'_>> {
    let parser = wasmparser::Parser::new(0);
    let mut parser = parser.parse_all(bytes);
    let mut segments = vec![];
    let mut data_range = 0..0;
    let mut symbols = vec![];

    // Process the payloads in the raw wasm file so we can extract the specific sections we need
    while let Some(payload) = parser.next() {
        let payload = payload.context("failed to parse wasm while reading data section")?;
        match payload {
            Payload::DataSection(section) => {
                data_range = section.range();
                segments = section.into_iter().collect::<Result<Vec<_>, _>>()?
            }
            Payload::CustomSection(section) if section.name() == "linking" => {
                let reader = BinaryReader::new(section.data(), 0);
                let reader = LinkingSectionReader::new(reader)?;
                for subsection in reader.subsections() {
                    if let Linking::SymbolTable(map) = subsection? {
                        symbols = map.into_iter().collect::<Result<Vec<_>, _>>()?;
                    }
                }
            }
            _ => {}
        }
    }

    // Accumulate the data symbols into a btreemap for later use
    let mut data_symbols = BTreeMap::new();
    for (index, symbol) in symbols.iter().enumerate() {
        let SymbolInfo::Data {
            symbol: Some(symbol),
            ..
        } = symbol
        else {
            continue;
        };

        let data_segment = segments
            .get(symbol.index as usize)
            .context("Failed to find data segment")?;
        let offset: usize =
            data_segment.range.end - data_segment.data.len() + (symbol.offset as usize);
        let range = offset..(offset + symbol.size as usize);

        data_symbols.insert(
            index,
            DataSymbol {
                index,
                range,
                segment_offset: symbol.offset as usize,
                symbol_size: symbol.size as usize,
                which_data_segment: symbol.index as usize,
            },
        );
    }

    Ok(RawDataSection {
        data_range,
        symbols,
        data_symbols,
    })
}

fn checked_data_symbol_range(segment_len: usize, offset: usize, size: usize) -> Result<Range<usize>> {
    let end = offset.checked_add(size).context("data symbol range overflows")?;
    anyhow::ensure!(end <= segment_len, "data symbol range {offset}..{end} exceeds segment length {segment_len}");
    Ok(offset..end)
}

fn validate_mangled_function_mapping<'a>(
    original_names: impl IntoIterator<Item = &'a String>,
    bindgened_names: impl IntoIterator<Item = &'a String>,
) -> Result<()> {
    let original_mangled: HashSet<&str> = original_names
        .into_iter()
        .map(String::as_str)
        .filter(|name| is_rust_mangled(name))
        .collect();
    if original_mangled.is_empty() {
        return Ok(());
    }
    let bindgened: HashSet<&str> = bindgened_names.into_iter().map(String::as_str).collect();
    let mapped = original_mangled.iter().filter(|name| bindgened.contains(**name)).count();
    anyhow::ensure!(
        mapped > 0,
        "no original Rust-mangled functions map to bindgen output; build with --no-demangle"
    );
    Ok(())
}

fn is_rust_mangled(name: &str) -> bool {
    name.starts_with("_R") || name.starts_with("_ZN")
}

fn validate_table_abi(tables: &[(bool, bool, u64, Option<u64>)]) -> Result<()> {
    let funcref: Vec<_> = tables.iter().filter(|(is_funcref, _, _, _)| *is_funcref).collect();
    anyhow::ensure!(funcref.len() == 1, "wasm-split requires exactly one funcref table; found {}", funcref.len());
    let (_, table64, initial, maximum) = **funcref.first().expect("one table checked");
    anyhow::ensure!(!table64, "wasm-split does not support table64");
    anyhow::ensure!(maximum.is_some(), "wasm-split requires a bounded funcref table");
    anyhow::ensure!(Some(initial) == maximum, "wasm-split requires funcref table initial == maximum");
    Ok(())
}

fn validate_memory_abi(memories64: &[bool]) -> Result<()> {
    anyhow::ensure!(memories64.len() == 1, "wasm-split requires exactly one memory; found {}", memories64.len());
    anyhow::ensure!(!memories64[0], "wasm-split does not support memory64");
    Ok(())
}

fn validate_active_data_ranges(ranges: &[(u64, u64)]) -> Result<()> {
    let mut ranges = ranges.iter().copied().filter(|(start, end)| start < end).collect::<Vec<_>>();
    ranges.sort_unstable();
    let mut prior_end = 0;
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        anyhow::ensure!(end <= u32::MAX as u64 + 1, "active data segment range exceeds wasm32 address space");
        if index > 0 {
            anyhow::ensure!(start >= prior_end, "overlapping active data segments at {start:#x}..{end:#x}");
        }
        prior_end = end;
    }
    Ok(())
}

fn validate_supported_abi(module: &Module) -> Result<()> {
    let tables = module.tables.iter().map(|table| (
        table.element_ty == RefType::FUNCREF,
        table.table64,
        table.initial,
        table.maximum,
    )).collect::<Vec<_>>();
    validate_table_abi(&tables)?;

    let memories64 = module.memories.iter().map(|memory| memory.memory64).collect::<Vec<_>>();
    validate_memory_abi(&memories64)?;
    let memory = module.memories.iter().next().expect("one memory validated").id();

    let mut active_ranges = Vec::new();
    for (index, data) in module.data.iter().enumerate() {
        let DataKind::Active { memory: data_memory, offset } = &data.kind else {
            anyhow::bail!("wasm-split does not support passive data segment {index}");
        };
        anyhow::ensure!(*data_memory == memory, "data segment {index} targets an unsupported memory");
        let ConstExpr::Value(ir::Value::I32(base)) = offset else {
            anyhow::bail!("wasm-split requires constant i32 offset for data segment {index}");
        };
        let start = *base as u32 as u64;
        let end = start.checked_add(data.value.len() as u64)
            .context("active data segment address range overflow")?;
        active_ranges.push((start, end));
    }
    validate_active_data_ranges(&active_ranges)
}

fn add_data_alias_edges(graph: &mut HashMap<Node, HashSet<Node>>, symbols: &BTreeMap<usize, DataSymbol>) -> usize {
    let mut by_segment = BTreeMap::<usize, Vec<(Range<usize>, usize)>>::new();
    for symbol in symbols.values().filter(|symbol| symbol.symbol_size > 0) {
        by_segment.entry(symbol.which_data_segment).or_default().push((
            symbol.segment_offset..symbol.segment_offset + symbol.symbol_size,
            symbol.index,
        ));
    }
    let mut edge_count = 0;
    for ranges in by_segment.values_mut() {
        ranges.sort_by_key(|(range, id)| (range.start, range.end, *id));
        for i in 0..ranges.len() {
            let (left, left_id) = &ranges[i];
            for (right, right_id) in ranges.iter().skip(i + 1) {
                if right.start >= left.end { break; }
                if left.start < right.end && right.start < left.end {
                    graph.entry(Node::DataSymbol(*left_id)).or_default().insert(Node::DataSymbol(*right_id));
                    graph.entry(Node::DataSymbol(*right_id)).or_default().insert(Node::DataSymbol(*left_id));
                    edge_count += 2;
                }
            }
        }
    }
    edge_count
}

fn function_symbol_index(symbol: &SymbolInfo<'_>, function_count: usize) -> Result<Option<usize>> {
    match symbol {
        SymbolInfo::Func { index, .. } => {
            let index = *index as usize;
            anyhow::ensure!(index < function_count, "function symbol index {index} exceeds original function count {function_count}");
            Ok(Some(index))
        }
        _ => Ok(None),
    }
}

/// Return every symbol range intersecting the relocation's patched byte span.
/// Relocation ranges and symbol ranges are half-open. Intersecting rather than
/// requiring full containment is conservative for aliases smaller than a LEB
/// relocation's encoded width.
fn overlapping_ranges(ranges: &[(Range<usize>, usize)], prefix_max_ends: &[usize], reloc: Range<usize>) -> Vec<usize> {
    if reloc.start >= reloc.end {
        return Vec::new();
    }
    let mut index = ranges.partition_point(|(range, _)| range.start < reloc.end);
    let mut owners = Vec::new();
    while index > 0 {
        index -= 1;
        if prefix_max_ends[index] <= reloc.start {
            break;
        }
        let (range, _) = &ranges[index];
        if reloc.start < range.end && range.start < reloc.end {
            owners.push(index);
        }
    }
    owners
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn subtract_ranges(range: Range<usize>, cuts: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut remaining = Vec::new();
    let mut cursor = range.start;
    for cut in cuts {
        if cut.end <= cursor { continue; }
        if cut.start >= range.end { break; }
        if cursor < cut.start {
            remaining.push(cursor..cut.start.min(range.end));
        }
        cursor = cursor.max(cut.end);
        if cursor >= range.end { break; }
    }
    if cursor < range.end { remaining.push(cursor..range.end); }
    remaining
}

#[cfg(test)]
mod graph_bridge_tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum TestNode {
        Function(u8),
        Data(u8),
    }

    fn add_edge(graph: &mut HashMap<TestNode, HashSet<TestNode>>, from: TestNode, to: TestNode) {
        graph.entry(from).or_default().insert(to);
    }

    #[test]
    fn bridges_missing_chains_cycles_multiple_parents_and_data_nodes() {
        use TestNode::{Data, Function};

        let p1 = Function(1);
        let p2 = Function(2);
        let gap_a = Function(90);
        let gap_b = Function(91);
        let leaf = Function(3);
        let data = Data(4);
        let mut graph = HashMap::new();
        add_edge(&mut graph, p1, gap_a);
        add_edge(&mut graph, p2, gap_a);
        add_edge(&mut graph, gap_a, gap_b);
        add_edge(&mut graph, gap_b, gap_a); // Missing-node cycle.
        add_edge(&mut graph, gap_b, leaf);
        add_edge(&mut graph, leaf, data);
        add_edge(&mut graph, data, gap_b); // Cycle through a mapped data node.

        let mapped = |node| match node {
            Function(90 | 91) => None,
            other => Some(other),
        };
        let bridged = bridge_relocation_graph(&graph, mapped);

        assert_eq!(bridged[&p1], HashSet::from([leaf]));
        assert_eq!(bridged[&p2], HashSet::from([leaf]));
        assert_eq!(bridged[&leaf], HashSet::from([data]));
        assert_eq!(bridged[&data], HashSet::from([leaf]));
    }

    #[test]
    fn disconnected_nodes_do_not_become_reachable_from_split_roots() {
        use TestNode::Function;

        let root = Function(1);
        let gap = Function(90);
        let leaf = Function(2);
        let isolated = Function(7);
        let isolated_leaf = Function(8);
        let mut graph = HashMap::new();
        add_edge(&mut graph, root, gap);
        add_edge(&mut graph, gap, leaf);
        add_edge(&mut graph, isolated, gap);
        add_edge(&mut graph, isolated, isolated_leaf);

        let bridged = bridge_relocation_graph(&graph, |node| {
            (node != gap).then_some(node)
        });
        let reachable = reachable_graph(&bridged, &HashSet::from([root]));

        assert_eq!(bridged[&root], HashSet::from([leaf]));
        assert!(reachable.contains(&leaf));
        assert!(!reachable.contains(&isolated));
        assert!(!reachable.contains(&isolated_leaf));
    }

    #[test]
    fn zero_sized_data_symbol_has_an_empty_valid_range() {
        assert_eq!(checked_data_symbol_range(16, 16, 0).unwrap(), 16..16);
        assert_eq!(checked_data_symbol_range(16, 12, 4).unwrap(), 12..16);
        assert!(checked_data_symbol_range(16, 17, 0).is_err());
        assert!(checked_data_symbol_range(16, 15, 2).is_err());
    }

    #[test]
    fn data_relocation_belongs_to_every_overlapping_symbol() {
        let ranges = vec![(0..16, 10), (4..8, 11), (4..8, 12), (8..12, 13)];
        let prefix_max_ends = vec![16, 16, 16, 16];
        let owners = overlapping_ranges(&ranges, &prefix_max_ends, 4..8)
            .into_iter()
            .map(|index| ranges[index].1)
            .collect::<HashSet<_>>();
        assert_eq!(owners, HashSet::from([10, 11, 12]));
    }

    #[test]
    fn relocation_span_may_overlap_symbol_boundary_conservatively() {
        let ranges = vec![(0..4, 10), (4..8, 11)];
        let prefix_max_ends = vec![4, 8];
        assert_eq!(overlapping_ranges(&ranges, &prefix_max_ends, 2..6), vec![1, 0]);
        assert_eq!(overlapping_ranges(&ranges, &prefix_max_ends, 0..4), vec![0]);
        assert_eq!(overlapping_ranges(&ranges, &prefix_max_ends, 4..8), vec![1]);
    }

    #[test]
    fn subtraction_preserves_bytes_owned_by_live_aliases() {
        let cuts = merge_ranges(vec![8..16, 0..4, 3..10]);
        assert_eq!(cuts, vec![0..16]);
        assert_eq!(subtract_ranges(2..20, &cuts), vec![16..20]);
        assert_eq!(subtract_ranges(20..24, &cuts), vec![20..24]);
    }

    #[test]
    fn live_short_alias_protects_its_bytes_from_larger_cold_alias() {
        // Reproduces the observed layout: live symbol 68723 is at 217..220,
        // while cold alias 100071 covers 212..220.
        let main_owned = merge_ranges(vec![217..220]);
        assert_eq!(subtract_ranges(212..220, &main_owned), vec![212..217]);
        assert!(subtract_ranges(217..220, &main_owned).is_empty());
    }

    #[test]
    fn function_symbol_resolves_by_index_even_when_name_is_an_alias() {
        let symbol = SymbolInfo::Func { flags: wasmparser::SymbolFlags::empty(), index: 1, name: Some("alias-name-not-in-module") };
        assert_eq!(function_symbol_index(&symbol, 2).unwrap(), Some(1));
        assert!(function_symbol_index(&symbol, 1).is_err());
    }

    #[test]
    fn overlapping_data_aliases_share_reachability_in_both_directions() {
        let symbols = BTreeMap::from([
            (1, DataSymbol { index: 1, range: 100..108, segment_offset: 0, symbol_size: 8, which_data_segment: 0 }),
            (2, DataSymbol { index: 2, range: 104..108, segment_offset: 4, symbol_size: 4, which_data_segment: 0 }),
            (3, DataSymbol { index: 3, range: 108..112, segment_offset: 8, symbol_size: 4, which_data_segment: 0 }),
            (4, DataSymbol { index: 4, range: 104..108, segment_offset: 4, symbol_size: 4, which_data_segment: 1 }),
        ]);
        let mut graph = HashMap::new();
        assert_eq!(add_data_alias_edges(&mut graph, &symbols), 2);
        let from_outer = reachable_graph(&graph, &HashSet::from([Node::DataSymbol(1)]));
        let from_inner = reachable_graph(&graph, &HashSet::from([Node::DataSymbol(2)]));
        assert!(from_outer.contains(&Node::DataSymbol(2)));
        assert!(from_inner.contains(&Node::DataSymbol(1)));
        assert!(!from_outer.contains(&Node::DataSymbol(3))); // touching ranges do not overlap
        assert!(!from_outer.contains(&Node::DataSymbol(4))); // separate segment
    }

    #[test]
    fn rejects_funcref_table_shapes_expand_ifunc_cannot_support() {
        assert!(validate_table_abi(&[(true, false, 8, Some(8))]).is_ok());
        assert!(validate_table_abi(&[(true, false, 8, Some(16))]).is_err());
        assert!(validate_table_abi(&[(true, false, 8, None)]).is_err());
        assert!(validate_table_abi(&[(true, false, 8, Some(8)), (true, false, 8, Some(8))]).is_err());
        assert!(validate_table_abi(&[(false, false, 8, Some(8))]).is_err());
        assert!(validate_table_abi(&[(true, true, 8, Some(8))]).is_err());
    }

    #[test]
    fn rejects_memory64_and_multiple_memories() {
        assert!(validate_memory_abi(&[false]).is_ok());
        assert!(validate_memory_abi(&[true]).is_err());
        assert!(validate_memory_abi(&[false, false]).is_err());
        assert!(validate_memory_abi(&[]).is_err());
    }

    #[test]
    fn rejects_overlapping_active_data_segments_but_allows_touching_ranges() {
        assert!(validate_active_data_ranges(&[(0, 8), (8, 16)]).is_ok());
        assert!(validate_active_data_ranges(&[(0, 8), (7, 12)]).is_err());
        assert!(validate_active_data_ranges(&[(0, u32::MAX as u64 + 2)]).is_err());
    }

    #[test]
    fn requires_some_original_rust_mangled_names_to_survive_bindgen() {
        let original = vec!["_Rlegacy".to_owned(), "_ZNlegacy".to_owned(), "plain".to_owned()];
        assert!(validate_mangled_function_mapping(original.iter(), std::iter::empty()).is_err());
        let retained = "_Rlegacy".to_owned();
        assert!(validate_mangled_function_mapping(original.iter(), [&retained]).is_ok());
        let plain = "plain".to_owned();
        assert!(validate_mangled_function_mapping([&plain], std::iter::empty()).is_ok());
    }
}
