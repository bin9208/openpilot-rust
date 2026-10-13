#[cfg(feature = "native")]
fn main() {
    use std::{env, path::PathBuf};
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../../..");
    let version =
        std::fs::read_to_string(root.join("openpilot/common/version.h")).expect("source version");
    println!(
        "cargo:rustc-env=ENCODER_VERSION={}",
        version.split('"').nth(1).expect("COMMA_VERSION")
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("openpilot/common/version.h").display()
    );
    let source =
        env::var("DEP_OPENPILOT_JPEG_SOURCE_INCLUDE").expect("external JPEG source headers");
    let build =
        env::var("DEP_OPENPILOT_JPEG_BUILD_INCLUDE").expect("external JPEG configured headers");
    let mut bindings = bindgen::Builder::default()
        .header_contents(
            "encoder_jpeg.h",
            "#include <stdio.h>\n#include <jpeglib.h>\n",
        )
        .clang_arg(format!("-I{source}"))
        .clang_arg(format!("-I{build}"))
        .allowlist_function("jpeg_.*")
        .allowlist_type("jpeg_.*|J_COLOR_SPACE")
        .allowlist_var("JPEG_LIB_VERSION|JCS_.*")
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false);
    let target = env::var("TARGET").expect("target triple");
    if env::var("HOST").expect("host triple") != target {
        bindings = bindings.clang_arg(format!("--target={target}"));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("output directory"));
    bindings
        .generate()
        .expect("external libjpeg ABI bindings")
        .write_to_file(output.join("jpeg.rs"))
        .expect("write libjpeg ABI bindings");
    let kernel = root.join("third_party/linux/include");
    let mut header = format!(
        "#include <stddef.h>\n#include <sys/time.h>\n#include \"{}/v4l2-controls.h\"\n#include <linux/videodev2.h>\n#include <linux/ion.h>\n#include <linux/msm_ion.h>\n",
        kernel.display()
    );
    let qcom_header = root.join("openpilot/tools/replay/qcom_decoder.h");
    let qcom = std::fs::read_to_string(&qcom_header).expect("pinned source Qualcomm decoder UAPI");
    for constant in [
        "V4L2_EVENT_MSM_VIDC_START",
        "V4L2_EVENT_MSM_VIDC_FLUSH_DONE",
        "V4L2_EVENT_MSM_VIDC_PORT_SETTINGS_CHANGED_INSUFFICIENT",
        "V4L2_QCOM_CMD_FLUSH_CAPTURE",
        "V4L2_QCOM_CMD_FLUSH",
    ] {
        let prefix = format!("#define {constant} ");
        let expression = qcom
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .expect("source Qualcomm decoder ABI definition");
        header.push_str(&format!("#ifdef {constant}\n#if {constant} != ({expression})\n#error Qualcomm_decoder_ABI_differs_from_pinned_source\n#endif\n#else\n#define {constant} {expression}\n#endif\n"));
    }
    println!("cargo:rerun-if-changed={}", qcom_header.display());
    let cluster_encoder = root.join("openpilot/system/loggerd/encoder/cluster_h264_encoder.cc");
    let encoder_text =
        std::fs::read_to_string(&cluster_encoder).expect("pinned source cluster encoder flags");
    for name in ["V4L2_QCOM_BUF_FLAG_CODECCONFIG", "V4L2_QCOM_BUF_FLAG_EOS"] {
        let prefix = format!("#define {name} ");
        let value = encoder_text
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .expect("source cluster encoder ABI definition");
        header.push_str(&format!("#ifdef {name}\n#if {name} != ({value})\n#error Qualcomm_encoder_ABI_differs_from_pinned_source\n#endif\n#else\n#define {name} {value}\n#endif\n"));
    }
    println!("cargo:rerun-if-changed={}", cluster_encoder.display());
    for request in [
        "VIDIOC_QUERYCAP",
        "VIDIOC_ENUM_FMT",
        "VIDIOC_S_FMT",
        "VIDIOC_G_FMT",
        "VIDIOC_S_PARM",
        "VIDIOC_S_SELECTION",
        "VIDIOC_S_CTRL",
        "VIDIOC_G_CTRL",
        "VIDIOC_S_EXT_CTRLS",
        "VIDIOC_SUBSCRIBE_EVENT",
        "VIDIOC_DQEVENT",
        "VIDIOC_DECODER_CMD",
        "VIDIOC_REQBUFS",
        "VIDIOC_STREAMON",
        "VIDIOC_STREAMOFF",
        "VIDIOC_QBUF",
        "VIDIOC_DQBUF",
        "VIDIOC_ENCODER_CMD",
        "ION_IOC_ALLOC",
        "ION_IOC_SHARE",
        "ION_IOC_FREE",
        "ION_IOC_CUSTOM",
        "ION_IOC_INV_CACHES",
        "ION_IOC_CLEAN_CACHES",
    ] {
        header.push_str(&format!(
            "static const unsigned long ENCODER_{request} = {request};\n"
        ));
    }
    for constant in [
        "V4L2_PIX_FMT_HEVC",
        "V4L2_PIX_FMT_H264",
        "V4L2_PIX_FMT_NV12",
        "ION_IOMMU_HEAP_ID",
    ] {
        header.push_str(&format!(
            "static const unsigned int ENCODER_{constant} = {constant};\n"
        ));
    }
    let mut v4l = bindgen::Builder::default()
        .header_contents("encoder_v4l.h", &header)
        .clang_arg(format!("-I{}", kernel.display()))
        .allowlist_type("v4l2_.*|ion_.*")
        .allowlist_var("V4L2_.*|ION_.*|ENCODER_.*")
        .prepend_enum_name(false)
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false);
    if env::var("HOST").expect("host triple") != target {
        v4l = v4l.clang_arg(format!("--target={target}"));
    }
    v4l.generate()
        .expect("V4L2 and ION kernel ABI bindings")
        .write_to_file(output.join("v4l.rs"))
        .expect("write kernel ABI bindings");
    println!("cargo:rerun-if-changed={}", kernel.display());
    println!("cargo:rustc-link-search=native={build}");
    println!("cargo:rustc-link-lib=static=jpeg");
    let yuv = PathBuf::from(
        env::var_os("ENCODER_LIBYUV_LIB").expect("explicit external libyuv library directory"),
    );
    assert!(
        yuv.join("libyuv.a").is_file(),
        "external libyuv archive is missing"
    );
    println!("cargo:rustc-link-search=native={}", yuv.display());
    println!("cargo:rustc-link-lib=static=yuv");
    println!("cargo:rerun-if-env-changed=ENCODER_LIBYUV_LIB");
    if let Ok(libraries) = env::var("ENCODER_FFMPEG_EXTRA_LIBS") {
        println!("cargo:rustc-link-arg=-Wl,--start-group");
        for library in libraries.split(',').filter(|name| !name.is_empty()) {
            assert!(
                library
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
                "invalid external FFmpeg library name"
            );
            // GNU ld needs the codec dependencies after ffmpeg-sys's archive.
            println!("cargo:rustc-link-arg=-l:lib{library}.a");
        }
        println!("cargo:rustc-link-arg=-Wl,--end-group");
    }
    println!("cargo:rerun-if-env-changed=ENCODER_FFMPEG_EXTRA_LIBS");
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed={build}");
}

#[cfg(not(feature = "native"))]
fn main() {}
