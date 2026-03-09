const LATO_V22_LATIN_REGULAR: &[u8] = include_bytes!("data/lato-v22-latin-regular.woff2");

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_temp_path(filename: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("woff2-rs-{unique}-{filename}"))
}

#[test]
fn converts_font_file_to_ttf_file() {
    let input_path = unique_temp_path("input.woff2");
    let output_path = unique_temp_path("output.ttf");

    fs::write(&input_path, LATO_V22_LATIN_REGULAR).unwrap();

    let input = fs::read(&input_path).unwrap();
    let ttf = woff2::convert_woff2_to_ttf(&mut input.as_slice()).unwrap();
    fs::write(&output_path, &ttf).unwrap();

    let written_ttf = fs::read(&output_path).unwrap();
    assert_eq!(None, ttf_parser::fonts_in_collection(&written_ttf));
    let _parsed_ttf = ttf_parser::Face::parse(&written_ttf, 0).unwrap();

    let _ = fs::remove_file(input_path);
    let _ = fs::remove_file(output_path);
}
