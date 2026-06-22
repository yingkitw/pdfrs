use std::fs;
fn main() {
    let bytes = fs::read("/Users/yingkitw/Desktop/myproject/pdfrs/target/test_output/signed_test.pdf").unwrap();
    let text = String::from_utf8_lossy(&bytes);
    println!("Looking for /Type /Sig: {}", text.contains("/Type /Sig"));
    println!("Looking for /Sig: {}", text.contains("/Sig"));
    for line in text.lines() {
        if line.contains("Sig") || line.contains("Contents") {
            println!("{}", line);
        }
    }
}
