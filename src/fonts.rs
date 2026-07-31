use eframe::egui;

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
    ];
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            let key = "jp".to_owned();
            fonts
                .font_data
                .insert(key.clone(), egui::FontData::from_owned(data));
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                fam.insert(0, key.clone());
            }
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                fam.push(key);
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}
