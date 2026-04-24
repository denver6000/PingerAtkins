fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/app-icon.ico");
        res.set("ProductName", "Network Overlay");
        res.set(
            "FileDescription",
            "Windows network reachability overlay for internet and site monitoring",
        );
        res.set("CompanyName", "Jaybien OJT");
        res.set("InternalName", "network-overlay");
        res.set("OriginalFilename", "internet-mon-jaybien.exe");
        res.set("LegalCopyright", "Copyright (C) 2026 Jaybien OJT");
        res.set("FileVersion", "0.1.0.0");
        res.set("ProductVersion", "0.1.0.0");
        res.compile().expect("failed to compile Windows resources");
    }
}
