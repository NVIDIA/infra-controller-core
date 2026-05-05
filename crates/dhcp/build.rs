/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

// dhcp package is built for x86_64 and aarch64 architectures
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
fn main() {
    let kea_shim_root = format!("{}/src/kea", env!("CARGO_MANIFEST_DIR"));

    let kea = pkg_config::Config::new()
        .cargo_metadata(true)
        .probe("kea")
        .expect("kea pkg-config not found");

    let kea_include_path = kea
        .include_paths
        .first()
        .map(|p| p.to_string_lossy().into_owned())
        .expect("kea pkg-config returned no include paths");

    cbindgen::Builder::new()
        .with_crate(env!("CARGO_MANIFEST_DIR"))
        .with_config(cbindgen::Config::from_file("cbindgen.toml").expect("Config file missing"))
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(format!("{kea_shim_root}/carbide_rust.h"));

    cc::Build::new()
        .cpp(true)
        .file(format!("{kea_shim_root}/logger.cc"))
        .file(format!("{kea_shim_root}/loader.cc"))
        .file(format!("{kea_shim_root}/callouts.cc"))
        .file(format!("{kea_shim_root}/carbide_logger.cc"))
        .include(kea_include_path)
        .pic(true)
        .compile("keashim");

    println!("cargo:rerun-if-changed=src/kea/callouts.cc");
    println!("cargo:rerun-if-changed=src/kea/callouts.h");
    println!("cargo:rerun-if-changed=src/kea/loader.cc");
    println!("cargo:rerun-if-changed=src/kea/logger.cc");
    println!("cargo:rerun-if-changed=src/kea/carbide_rust.h");
    println!("cargo:rerun-if-changed=src/kea/carbide_logger.cc");
    println!("cargo:rerun-if-changed=src/kea/carbide_logger.h");

    println!("cargo:rustc-link-lib=keashim");
    println!("cargo:rustc-link-lib=stdc++");
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn main() {}
