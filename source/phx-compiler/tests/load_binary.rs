//! `load_project_binary` decode and optional verify-on-load.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PcSpanTable, TypeTable, VerifyError,
};
use phx_compiler::{
    BuildError, BuildOptions, LoadOptions, ProjectConfig, build_project, load_project_binary,
    load_project_binary_with_options,
};

fn temp_bin_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phx_load_bin_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("phoenix.toml"),
        r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false
"#,
    )
    .unwrap();
    std::fs::write(dir.join("src/main.phx"), "main :: () => { };").unwrap();
    dir
}

fn malformed_module_bytes() -> Vec<u8> {
    let mut code = Vec::new();
    code.extend(
        Instruction {
            opcode: Opcode::Jump,
            operands: vec![],
        }
        .encode()
        .expect("encode"),
    );
    code.extend(
        Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        }
        .encode()
        .expect("encode"),
    );
    let module = BytecodeModule {
        header: FileHeader::new(5, 0),
        constants: ConstPool {
            entries: vec![ConstEntry {
                tag: ConstTag::SignedInt,
                payload: 1i32.to_le_bytes().to_vec(),
            }],
        },
        types: TypeTable::default(),
        functions: FunctionTable {
            functions: vec![FunctionRecord {
                function_id: 0,
                name_symbol_id: 0,
                arity: 0,
                local_count: 0,
                stack_max: 4,
                flags: 0,
                code_offset: 0,
                code_len: u32::try_from(code.len()).unwrap_or(0),
                return_type_id: 0,
            }],
        },
        code,
        local_layouts: LocalLayoutTable::default(),
        pc_spans: PcSpanTable::default(),
    };
    module.encode().expect("encode malformed module")
}

#[test]
fn default_load_skips_verify_for_valid_binary() {
    let dir = temp_bin_project("default");
    let config = ProjectConfig::load(&dir).unwrap();
    build_project(&config, None, BuildOptions::default()).expect("build");
    let module = load_project_binary(&config).expect("load without verify");
    assert_eq!(module.header.entry_function_id, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_on_load_accepts_valid_binary() {
    let dir = temp_bin_project("verify_ok");
    let config = ProjectConfig::load(&dir).unwrap();
    build_project(&config, None, BuildOptions::default()).expect("build");
    let options = LoadOptions {
        verify_on_load: true,
    };
    let module =
        load_project_binary_with_options(&config, options).expect("load with verify-on-load");
    assert_eq!(module.header.entry_function_id, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_on_load_rejects_malformed_binary() {
    let dir = temp_bin_project("verify_reject");
    let config = ProjectConfig::load(&dir).unwrap();
    build_project(&config, None, BuildOptions::default()).expect("build");
    let bin_path = config.build_root().join("bin").join("demo.phx0");
    std::fs::write(&bin_path, malformed_module_bytes()).expect("write malformed bin");
    let options = LoadOptions {
        verify_on_load: true,
    };
    let err = load_project_binary_with_options(&config, options).unwrap_err();
    match err {
        BuildError::Verify(VerifyError::MalformedInstruction { function_id, .. }) => {
            assert_eq!(function_id, 0);
        }
        other => panic!("expected verify failure, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
