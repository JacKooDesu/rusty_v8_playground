fn main() {
    // 連結 Windows 系統庫，解決 rusty_v8 相關的外部符號
    println!("cargo:rustc-link-lib=advapi32");

    println!("cargo:rustc-link-lib=wevtapi");
}
