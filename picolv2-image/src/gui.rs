use std::{
    env, fs,
    path::{Path, PathBuf},
};

use eframe::egui;

use crate::builder::{
    create_flash_image, image_to_uf2, load_firmware, CreateOptions, OutputFormat,
    UF2_PAYLOAD_SIZE,
};

pub fn run() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([740.0, 640.0])
            .with_min_inner_size([580.0, 480.0])
            .with_title("PicoLV2 Image Creator"),
        ..Default::default()
    };
    eframe::run_native(
        "PicoLV2 Image Creator",
        native_options,
        Box::new(|_cc| Ok(Box::new(PicoImageApp::new()))),
    )
}

struct PicoImageApp {
    firmware_path: String,
    patch_path: String,
    search_path: String,
    output_path: String,
    output_format: OutputFormat,
    auto_output_set: bool,
    status_message: Option<(String, bool)>, // (message, is_error)
    log_messages: Vec<String>,
}

impl PicoImageApp {
    fn new() -> Self {
        let search_path = env::var("PICOLV2_PATH").unwrap_or_else(|_| "plugins/pico".to_string());
        Self {
            firmware_path: String::new(),
            patch_path: String::new(),
            search_path,
            output_path: String::new(),
            output_format: OutputFormat::Uf2,
            auto_output_set: false,
            status_message: None,
            log_messages: Vec::new(),
        }
    }

    fn update_suggested_output(&mut self) {
        if !self.auto_output_set && !self.output_path.is_empty() {
            return;
        }
        if self.patch_path.is_empty() {
            return;
        }
        let p = Path::new(&self.patch_path);
        let patch_name = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|name| {
                if let Some(stripped) = name.strip_suffix(".ingen") {
                    stripped
                } else {
                    name
                }
            })
            .unwrap_or("pico_patch");

        let ext = match self.output_format {
            OutputFormat::Uf2 => "uf2",
            OutputFormat::RawBinary => "bin",
        };

        let default_name = format!("{patch_name}.{ext}");
        let parent = p.parent().unwrap_or_else(|| Path::new("."));
        self.output_path = parent.join(default_name).to_string_lossy().to_string();
        self.auto_output_set = true;
    }

    fn build(&mut self) {
        self.status_message = None;

        let firmware = self.firmware_path.trim().to_string();
        let patch = self.patch_path.trim().to_string();
        let search = self.search_path.trim().to_string();
        let output = self.output_path.trim().to_string();
        let output_format = self.output_format;

        if firmware.is_empty() {
            self.set_error("Please select a firmware file (.elf).");
            return;
        }
        if patch.is_empty() {
            self.set_error("Please select an Ingen patch (.ingen folder).");
            return;
        }
        if search.is_empty() {
            self.set_error("Please specify a plugin search path (PICOLV2_PATH).");
            return;
        }
        if output.is_empty() {
            self.set_error("Please specify an output file path.");
            return;
        }

        self.log(format!("--- Starting build ({output_format:?}) ---"));
        self.log(format!("Firmware: {firmware}"));
        self.log(format!("Patch: {patch}"));
        self.log(format!("Plugin search path: {search}"));
        self.log(format!("Output file: {output}"));

        let fw_path = PathBuf::from(&firmware);
        let patch_path = PathBuf::from(&patch);
        let out_path = PathBuf::from(&output);

        if !fw_path.is_file() {
            self.set_error(&format!("Firmware file not found: {firmware}"));
            return;
        }
        if !patch_path.exists() {
            self.set_error(&format!("Patch path does not exist: {patch}"));
            return;
        }

        // Test loading firmware
        let fw_bytes = match load_firmware(&fw_path) {
            Ok(bytes) => {
                self.log(format!("Loaded firmware: {} bytes", bytes.len()));
                bytes
            }
            Err(err) => {
                self.set_error(&format!("Firmware error: {err}"));
                return;
            }
        };

        let options = CreateOptions {
            firmware_path: &fw_path,
            graph_path: &patch_path,
            search_path: &search,
            explicit_plugins: &[],
        };

        let result = match create_flash_image(&options) {
            Ok(res) => res,
            Err(err) => {
                self.set_error(&format!("Image creation failed: {err}"));
                return;
            }
        };

        self.log(format!(
            "Ingen graph compiled: {} nodes, {} edges",
            result.graph_nodes, result.graph_edges
        ));
        self.log(format!("Discovered {} plugins:", result.plugins.len()));
        for (i, uri) in result.plugins.iter().enumerate() {
            self.log(format!("  [{i}] {uri}"));
        }
        self.log(format!(
            "Bundle size: {} bytes (max 1048576 bytes)",
            result.bundle_size
        ));
        self.log(format!("Firmware size: {} bytes", fw_bytes.len()));

        let final_data = match output_format {
            OutputFormat::RawBinary => {
                self.log(format!(
                    "Writing raw flash image ({} bytes)...",
                    result.image_bytes.len()
                ));
                result.image_bytes
            }
            OutputFormat::Uf2 => {
                self.log("Converting flash image to UF2 format...".to_string());
                match image_to_uf2(&result.image_bytes) {
                    Ok(uf2_bytes) => {
                        let populated_blocks = result
                            .image_bytes
                            .chunks(UF2_PAYLOAD_SIZE)
                            .filter(|chunk| chunk.iter().any(|b| *b != 0xff))
                            .count();
                        self.log(format!(
                            "UF2 generated: {} bytes ({populated_blocks} flash blocks)",
                            uf2_bytes.len()
                        ));
                        uf2_bytes
                    }
                    Err(err) => {
                        self.set_error(&format!("UF2 conversion failed: {err}"));
                        return;
                    }
                }
            }
        };

        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                if let Err(err) = fs::create_dir_all(parent) {
                    self.set_error(&format!("Failed to create output directory: {err}"));
                    return;
                }
            }
        }

        if let Err(err) = fs::write(&out_path, &final_data) {
            self.set_error(&format!("Failed to write output file: {err}"));
            return;
        }

        let success_msg = format!("Successfully created {output} ({} bytes)", final_data.len());
        self.log(success_msg.clone());
        self.status_message = Some((success_msg, false));
    }

    fn set_error(&mut self, msg: &str) {
        self.log(format!("ERROR: {msg}"));
        self.status_message = Some((msg.to_string(), true));
    }

    fn log(&mut self, msg: String) {
        self.log_messages.push(msg);
    }
}

impl eframe::App for PicoImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("PicoLV2 Image Creator");
            ui.label("Build firmware images & UF2 files for PicoLV2 from Ingen patches.");
            ui.add_space(8.0);

            // Inputs section
            egui::Grid::new("inputs_grid")
                .num_columns(3)
                .spacing([10.0, 10.0])
                .min_col_width(120.0)
                .show(ui, |ui| {
                    // Firmware File
                    ui.label("Firmware File:");
                    let fw_edit = ui.add_sized(
                        [ui.available_width() - 100.0, 22.0],
                        egui::TextEdit::singleline(&mut self.firmware_path)
                            .hint_text("Path to firmware (.elf)"),
                    );
                    if fw_edit.changed() {
                        self.status_message = None;
                    }
                    if ui.button("Browse...").clicked() {
                        let mut dialog = rfd::FileDialog::new()
                            .add_filter("Firmware (*.elf)", &["elf", "ELF"]);
                        if !self.firmware_path.is_empty() {
                            if let Some(parent) = Path::new(&self.firmware_path).parent() {
                                dialog = dialog.set_directory(parent);
                            }
                        }
                        if let Some(path) = dialog.pick_file() {
                            self.firmware_path = path.to_string_lossy().to_string();
                            self.status_message = None;
                        }
                    }
                    ui.end_row();

                    // Patch Folder
                    ui.label("Patch (.ingen):");
                    let patch_edit = ui.add_sized(
                        [ui.available_width() - 100.0, 22.0],
                        egui::TextEdit::singleline(&mut self.patch_path)
                            .hint_text("Path to .ingen patch directory"),
                    );
                    if patch_edit.changed() {
                        self.status_message = None;
                        self.update_suggested_output();
                    }
                    if ui.button("Browse...").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if !self.patch_path.is_empty() {
                            if let Some(parent) = Path::new(&self.patch_path).parent() {
                                dialog = dialog.set_directory(parent);
                            }
                        }
                        if let Some(path) = dialog.pick_folder() {
                            self.patch_path = path.to_string_lossy().to_string();
                            self.status_message = None;
                            self.update_suggested_output();
                        }
                    }
                    ui.end_row();

                    // Search Path
                    ui.label("Plugin Path:");
                    ui.add_sized(
                        [ui.available_width() - 100.0, 22.0],
                        egui::TextEdit::singleline(&mut self.search_path)
                            .hint_text("Directory containing Pico LV2 bundles (PICOLV2_PATH)"),
                    );
                    if ui.button("Browse...").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if !self.search_path.is_empty() {
                            dialog = dialog.set_directory(&self.search_path);
                        }
                        if let Some(path) = dialog.pick_folder() {
                            self.search_path = path.to_string_lossy().to_string();
                        }
                    }
                    ui.end_row();

                    // Output Format
                    ui.label("Output Format:");
                    ui.horizontal(|ui| {
                        let prev_format = self.output_format;
                        ui.radio_value(&mut self.output_format, OutputFormat::Uf2, "UF2 Image (.uf2) (Default)");
                        ui.radio_value(&mut self.output_format, OutputFormat::RawBinary, "Raw Flash Image (.bin)");
                        if prev_format != self.output_format {
                            if self.auto_output_set {
                                self.update_suggested_output();
                            } else if !self.output_path.is_empty() {
                                let old_ext = match prev_format {
                                    OutputFormat::Uf2 => ".uf2",
                                    OutputFormat::RawBinary => ".bin",
                                };
                                let new_ext = match self.output_format {
                                    OutputFormat::Uf2 => ".uf2",
                                    OutputFormat::RawBinary => ".bin",
                                };
                                if self.output_path.ends_with(old_ext) {
                                    let base = self.output_path.trim_end_matches(old_ext);
                                    self.output_path = format!("{base}{new_ext}");
                                }
                            }
                        }
                    });
                    ui.label(""); // Empty cell for 3rd column
                    ui.end_row();

                    // Output File Path
                    ui.label("Output File:");
                    let out_edit = ui.add_sized(
                        [ui.available_width() - 100.0, 22.0],
                        egui::TextEdit::singleline(&mut self.output_path)
                            .hint_text("Destination output file path"),
                    );
                    if out_edit.changed() {
                        self.auto_output_set = false;
                        self.status_message = None;
                    }
                    if ui.button("Browse...").clicked() {
                        let ext = match self.output_format {
                            OutputFormat::Uf2 => "uf2",
                            OutputFormat::RawBinary => "bin",
                        };
                        let mut dialog = rfd::FileDialog::new().add_filter(
                            match self.output_format {
                                OutputFormat::Uf2 => "UF2 Image (*.uf2)",
                                OutputFormat::RawBinary => "Binary Image (*.bin, *.img)",
                            },
                            &[ext],
                        );
                        if !self.output_path.is_empty() {
                            let p = Path::new(&self.output_path);
                            if let Some(parent) = p.parent() {
                                dialog = dialog.set_directory(parent);
                            }
                            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                                dialog = dialog.set_file_name(name);
                            }
                        }
                        if let Some(path) = dialog.save_file() {
                            self.output_path = path.to_string_lossy().to_string();
                            self.auto_output_set = false;
                            self.status_message = None;
                        }
                    }
                    ui.end_row();
                });

            ui.add_space(10.0);

            // Action button and status banner
            ui.horizontal(|ui| {
                let btn_text = match self.output_format {
                    OutputFormat::Uf2 => "Generate UF2 Image",
                    OutputFormat::RawBinary => "Build Flash Image",
                };
                let btn = ui.add_sized([180.0, 32.0], egui::Button::new(btn_text));
                if btn.clicked() {
                    self.build();
                }

                if let Some((msg, is_err)) = &self.status_message {
                    let color = if *is_err {
                        egui::Color32::from_rgb(220, 80, 80)
                    } else {
                        egui::Color32::from_rgb(80, 200, 80)
                    };
                    ui.colored_label(color, msg);
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Build Log Header and Clear button
            ui.horizontal(|ui| {
                ui.strong("Build Log:");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear Log").clicked() {
                        self.log_messages.clear();
                        self.status_message = None;
                    }
                });
            });

            ui.add_space(4.0);

            // Scrollable Log Console Area
            egui::Frame::canvas(ui.style())
                .fill(egui::Color32::from_rgb(24, 24, 28))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            if self.log_messages.is_empty() {
                                ui.colored_label(
                                    egui::Color32::from_rgb(120, 120, 120),
                                    "Ready. Select firmware and patch, then click Generate.",
                                );
                            } else {
                                for line in &self.log_messages {
                                    let color = if line.starts_with("ERROR") {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    } else if line.starts_with("Successfully") {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else if line.starts_with("---") {
                                        egui::Color32::from_rgb(120, 180, 255)
                                    } else {
                                        egui::Color32::from_rgb(220, 220, 220)
                                    };
                                    ui.colored_label(color, egui::RichText::new(line).monospace());
                                }
                            }
                        });
                });
        });
    }
}
