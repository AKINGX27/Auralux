use auralux_core::conversion::{
    ffmpeg_args, parse_ffmpeg_progress, sanitize_file_name, ConversionFormat, ConversionJobRequest,
    ConversionPreset,
};
use std::path::Path;

#[test]
fn constructs_opus_conversion_arguments() {
    let request = ConversionJobRequest {
        source_path: "/music/source.flac".into(),
        output_dir: "/tmp".into(),
        preset: ConversionPreset {
            format: ConversionFormat::Opus,
            quality: Some("128k".into()),
        },
        overwrite: true,
    };

    let args = ffmpeg_args(&request, Path::new("/tmp/source.opus"));
    assert!(args
        .windows(2)
        .any(|pair| pair[0] == "-c:a" && pair[1] == "libopus"));
    assert!(args
        .windows(2)
        .any(|pair| pair[0] == "-b:a" && pair[1] == "128k"));
    assert!(args
        .windows(2)
        .any(|pair| pair[0] == "-progress" && pair[1] == "pipe:2"));
}

#[test]
fn parses_progress_lines() {
    assert_eq!(
        parse_ffmpeg_progress("progress=end", Some(10_000)),
        Some(1.0)
    );
    assert!(parse_ffmpeg_progress("out_time_ms=1500000", Some(10_000)).unwrap() >= 0.0);
    assert_eq!(parse_ffmpeg_progress("speed=1.0x", Some(10_000)), None);
}

#[test]
fn sanitizes_unsafe_output_filename_parts() {
    assert_eq!(sanitize_file_name(" bad/name?.flac "), "bad_name_.flac");
    assert_eq!(sanitize_file_name("..."), "untitled");
}
