use shadow_rs::ShadowBuilder;

fn main() {
    ShadowBuilder::builder()
        .deny_const(Default::default())
        .build()
        .expect("shadow build");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=../assets/brand/motionduo.ico");
        winresource::WindowsResource::new()
            .set_icon("../assets/brand/motionduo.ico")
            .set("ProductName", "MotionDuo")
            .set("FileDescription", "MotionDuo")
            .set("OriginalFilename", "motionduo.exe")
            .compile()
            .expect("compile Windows application resources");
    }
}
