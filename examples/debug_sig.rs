use std::fs;
fn main() {
    let bytes = fs::read("/Users/yingkitw/Desktop/myproject/pdfrs/target/test_output/signed_test.pdf").unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let re = regex::Regex::new(r"(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj").unwrap();
    println!("Found {} objects matching pattern", re.captures_iter(&text).count());
    for caps in re.captures_iter(&text) {
        println!("Object {}: has /Type /Sig? {}", &caps[1], caps[2].contains("/Type /Sig"));
    }
    // Show raw text around /Sig
    if let Some(pos) = text.find("/Type /Sig") {
        let start = pos.saturating_sub(50);
        let end = (pos + 200).min(text.len());
        println!("Context: {}", &text[start..end]);
    }
}
