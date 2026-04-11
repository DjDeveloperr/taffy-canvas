use std::{env, fs, path::PathBuf};

use skia_safe::{Data, EncodedImageFormat, Image};

fn main() {
    let input = PathBuf::from(env::args().nth(1).expect("input path"));
    let output = PathBuf::from(env::args().nth(2).expect("output path"));

    let bytes = fs::read(&input).expect("read input");
    let image = Image::from_encoded(Data::new_copy(&bytes)).expect("decode image");
    let encoded = image
        .encode(None, EncodedImageFormat::WEBP, Some(100))
        .expect("encode webp");

    fs::write(&output, encoded.as_bytes()).expect("write output");
    println!(
        "input_bytes={} output_bytes={}",
        bytes.len(),
        encoded.as_bytes().len()
    );
}
