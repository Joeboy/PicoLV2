use std::{env, fs, path::Path, process::ExitCode};

use crate::builder::{
    create_flash_image, firmware_from_elf_bytes, image_info, image_to_uf2, CreateOptions,
    UF2_PAYLOAD_SIZE,
};

pub fn run(arguments: &[String]) -> ExitCode {
    if arguments.first().map(String::as_str) == Some("uf2") {
        return uf2(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) == Some("info") {
        return info(&arguments[1..]);
    }
    if arguments.first().map(String::as_str) != Some("create") {
        eprintln!(
            "usage: PICOLV2_PATH=DIR[:DIR...] picolv2-image create -o IMAGE (--firmware-elf ELF | --firmware-bin BIN | --firmware FW) --ingen GRAPH.ingen [--plugin URI [...]]"
        );
        eprintln!("       picolv2-image uf2 -i IMAGE -o IMAGE.uf2");
        eprintln!("       picolv2-image info -i IMAGE");
        return ExitCode::from(2);
    }

    let mut output = None;
    let mut firmware_elf_path = None;
    let mut firmware_bin_path = None;
    let mut firmware_path = None;
    let mut graph_path = None;
    let mut plugins = Vec::new();
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-o" | "--output" => {
                index += 1;
                output = arguments.get(index).cloned();
            }
            "--firmware-elf" => {
                index += 1;
                firmware_elf_path = arguments.get(index).cloned();
            }
            "--firmware-bin" => {
                index += 1;
                firmware_bin_path = arguments.get(index).cloned();
            }
            "--firmware" => {
                index += 1;
                firmware_path = arguments.get(index).cloned();
            }
            "--graph" | "--ingen" => {
                index += 1;
                graph_path = arguments.get(index).cloned();
            }
            "--plugin" => {
                if index + 1 >= arguments.len() {
                    eprintln!("--plugin requires a URI");
                    return ExitCode::from(2);
                }
                plugins.push(arguments[index + 1].clone());
                index += 1;
            }
            argument => {
                eprintln!("unknown argument: {argument}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    let (Some(output), Some(graph_path)) = (output, graph_path) else {
        eprintln!("missing output or graph path");
        return ExitCode::from(2);
    };

    let firmware_path_buf = match (firmware_elf_path, firmware_bin_path, firmware_path) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
            eprintln!("cannot specify multiple firmware flags together");
            return ExitCode::from(2);
        }
        (Some(path), None, None) => {
            // Validate it parses as ELF now or let builder handle it
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(err) => return fail(&format!("cannot read {path}: {err}")),
            };
            if let Err(err) = firmware_from_elf_bytes(&bytes, &path) {
                return fail(&err);
            }
            Path::new(&path).to_path_buf()
        }
        (None, Some(path), None) | (None, None, Some(path)) => Path::new(&path).to_path_buf(),
        (None, None, None) => {
            eprintln!("missing --firmware-elf, --firmware-bin, or --firmware");
            return ExitCode::from(2);
        }
    };

    let search_path = match env::var("PICOLV2_PATH") {
        Ok(path) if !path.is_empty() => path,
        _ => return fail("PICOLV2_PATH is not set"),
    };

    let options = CreateOptions {
        firmware_path: &firmware_path_buf,
        graph_path: Path::new(&graph_path),
        search_path: &search_path,
        explicit_plugins: &plugins,
    };

    let result = match create_flash_image(&options) {
        Ok(res) => res,
        Err(err) => return fail(&err),
    };

    if let Err(error) = fs::write(&output, &result.image_bytes) {
        return fail(&format!("cannot write {output}: {error}"));
    }
    println!("wrote {output}");
    ExitCode::SUCCESS
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(1)
}

fn uf2(arguments: &[String]) -> ExitCode {
    let mut input = None;
    let mut output = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-i" | "--input" => {
                index += 1;
                input = arguments.get(index).cloned();
            }
            "-o" | "--output" => {
                index += 1;
                output = arguments.get(index).cloned();
            }
            argument => return fail(&format!("unknown argument: {argument}")),
        }
        index += 1;
    }
    let (Some(input), Some(output)) = (input, output) else {
        return fail("uf2 requires input and output paths");
    };
    let image = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => return fail(&format!("cannot read {input}: {error}")),
    };

    let uf2_bytes = match image_to_uf2(&image) {
        Ok(bytes) => bytes,
        Err(err) => return fail(&err),
    };

    let populated_blocks = image
        .chunks(UF2_PAYLOAD_SIZE)
        .filter(|chunk| chunk.iter().any(|byte| *byte != 0xff))
        .count();

    match fs::write(&output, uf2_bytes) {
        Ok(()) => {
            println!("wrote {output} ({populated_blocks} blocks)");
            ExitCode::SUCCESS
        }
        Err(error) => fail(&format!("cannot write {output}: {error}")),
    }
}

fn info(arguments: &[String]) -> ExitCode {
    let mut input = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-i" | "--input" => {
                index += 1;
                input = arguments.get(index).cloned();
            }
            argument => return fail(&format!("unknown argument: {argument}")),
        }
        index += 1;
    }
    let Some(input) = input else {
        return fail("info requires an input path");
    };
    let image = match fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => return fail(&format!("cannot read {input}: {error}")),
    };
    match image_info(&image, &input) {
        Ok(info_text) => {
            print!("{info_text}");
            ExitCode::SUCCESS
        }
        Err(err) => fail(&err),
    }
}
