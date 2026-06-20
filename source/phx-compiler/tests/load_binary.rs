//! `load_project_binary` decode and optional verify-on-load.

mod support;

use std::path::PathBuf;

use phx_bytecode::{
    BytecodeModule, ConstEntry, ConstPool, ConstTag, FileHeader, FunctionRecord, FunctionTable,
    Instruction, LocalLayoutTable, Opcode, PcSpanTable, TypeTable, VerifyError,
};
use phx_compiler::{
    BuildError, BuildOptions, LoadOptions, ProjectConfig, build_project, load_project_binary,
    load_project_binary_with_options,
};
use support::{fs_create_dir_all, fs_write, test_encode, test_err, test_ok};

fn temp_bin_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phx_load_bin_test_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    fs_create_dir_all(&dir.join("src"), "create src dir");
    fs_write(
        &dir.join("phoenix.toml"),
        r#"
[project]
name = "demo"
type = "bin"
module_src = "src"
bundle_std = false
"#,
        "write phoenix.toml",
    );
    fs_write(
        &dir.join("src/main.phx"),
        "main :: () => { };",
        "write main.phx",
    );
    dir
}

fn malformed_module_bytes() -> Vec<u8> {
    let mut code = Vec::new();
    code.extend(test_encode(&Instruction {
        opcode: Opcode::Jump,
        operands: vec![],
    }));
    code.extend(test_encode(&Instruction {
        opcode: Opcode::Return,
        operands: vec![],
    }));
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
    test_ok(module.encode(), "encode malformed module")
}

#[test]
fn default_load_skips_verify_for_valid_binary() {
    let dir = temp_bin_project("default");
    let config = test_ok(ProjectConfig::load(&dir), "load config");
    test_ok(
        build_project(&config, None, BuildOptions::default()),
        "build",
    );
    let module = test_ok(load_project_binary(&config), "load without verify");
    assert_eq!(module.header.entry_function_id, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_on_load_accepts_valid_binary() {
    let dir = temp_bin_project("verify_ok");
    let config = test_ok(ProjectConfig::load(&dir), "load config");
    test_ok(
        build_project(&config, None, BuildOptions::default()),
        "build",
    );
    let options = LoadOptions {
        verify_on_load: true,
    };
    let module = test_ok(
        load_project_binary_with_options(&config, options),
        "load with verify-on-load",
    );
    assert_eq!(module.header.entry_function_id, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_on_load_rejects_malformed_binary() {
    let dir = temp_bin_project("verify_reject");
    let config = test_ok(ProjectConfig::load(&dir), "load config");
    test_ok(
        build_project(&config, None, BuildOptions::default()),
        "build",
    );
    let bin_path = config.build_root().join("bin").join("demo.phx0");
    fs_write(&bin_path, malformed_module_bytes(), "write malformed bin");
    let options = LoadOptions {
        verify_on_load: true,
    };
    let err = test_err(
        load_project_binary_with_options(&config, options),
        "load malformed",
    );
    match err {
        BuildError::Verify(VerifyError::MalformedInstruction { function_id, .. }) => {
            assert_eq!(function_id, 0);
        }
        other => panic!("expected verify failure, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
