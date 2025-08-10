use std::{ffi::OsString, fs, path::Path};

fn main() {
    println!("cargo::rerun-if-changed=acpica/source/");

    let temp_dir = tempdir::TempDir::new("acpica")
        .expect("failed to create temporary directory for ACPICA compilation");
    let source_dir = temp_dir.path().join("source/");
    let source_include_dir = source_dir.join("include/");
    let source_include_platform_dir = source_include_dir.join("platform/");
    let source_components_dir = source_dir.join("components/");

    prepare_temp_dir(source_dir.as_path(), source_include_platform_dir.as_path());
    patch_acrust_include(source_include_platform_dir.as_path());
    compile(
        source_components_dir.as_path(),
        source_include_dir.as_path(),
    );
}

fn prepare_temp_dir(source_dir: impl AsRef<Path>, source_include_platform_dir: impl AsRef<Path>) {
    fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
        fs::create_dir_all(dst.as_ref())?;

        src.as_ref().read_dir()?.try_for_each(|entry| {
            let entry = entry?;
            let entry_ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.as_ref().join(entry.file_name());

            if entry_ty.is_dir() {
                copy_dir_all(src_path, dst_path)?;
            } else {
                fs::copy(src_path, dst_path)?;
            }

            Ok(())
        })
    }

    // copy all of the APCPICA source to the temp dir
    copy_dir_all("acpica/source/", source_dir.as_ref())
        .expect("failed to copy ACPICA source files to temporary directory for compilation (do you need to initialize the submodule?)");

    // copy the custom platform header we've premade
    fs::copy(
        "acrust.h",
        source_include_platform_dir.as_ref().join("acrust.h"),
    )
    .expect("failed to copy `acrust.h` platform headers");
}

fn patch_acrust_include(source_include_platform_dir: impl AsRef<Path>) {
    let acenv_h_path = source_include_platform_dir.as_ref().join("acenv.h");

    let acenv_h = fs::read_to_string(acenv_h_path.as_path())
        .expect("could not find or read `source/include/platform/acenv.h`");

    let search_regex = regex::Regex::new(r"(?s)#if defined\(_LINUX\).+?#endif")
        .expect("failed to compile search regex");

    let acenv_h_patched = search_regex.replace(&acenv_h, r#"#include "acrust.h""#);

    if !acenv_h_patched.contains("#include \"acrust.h\"") {
        panic!(
            "acenv.h should have contained a section of platform-specific includes (or detection failed)"
        );
    }

    fs::write(acenv_h_path.as_path(), acenv_h_patched.as_bytes())
        .expect("failed to write patched `acenv.h`");
}

fn compile(source_components_dir: impl AsRef<Path>, source_include_dir: impl AsRef<Path>) {
    let component_files = fs::read_dir(source_components_dir.as_ref())
        .expect("source directory should contain a `components` sub-directory")
        .map(|component_dir| component_dir.expect("could not read component directory"))
        .filter(|component_dir| {
            // Exclude the debugger and disassembler dirs because they give 'undefined type' errors.
            // We'll consider fixing this if the need arises on the OS side.
            ![OsString::from("debugger"), OsString::from("disassembler")]
                .contains(&component_dir.file_name())
        })
        .flat_map(|component_dir| {
            fs::read_dir(component_dir.path())
                .expect("failed to read the files within the component directory")
                .map(|c_file| c_file.expect("failed to read C file from component directory"))
                .map(|c_file| c_file.path())
        });

    cc::Build::new()
        .warnings(false)
        .include(source_include_dir.as_ref())
        .define("ACPI_DEBUG_OUTPUT", None)
        .flag("-fno-stack-protector")
        // Get rid of annoying warning when compiling ACPICA.
        .flag("-Wno-format-truncation")
        .opt_level(1)
        .files(component_files)
        .compile("acpica_sys");
}
