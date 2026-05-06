use crate::providers::Offer;
use owo_colors::OwoColorize;

pub fn render_offers(offers: &[Offer]) {
    if offers.is_empty() {
        println!("(no offers matched)");
        return;
    }
    println!(
        "{:<10} {:<20} {:>5} {:>8} {:>6} {:>10} {:<6} {:>6}",
        "ID".bold(),
        "GPU".bold(),
        "N".bold(),
        "VRAM/GB".bold(),
        "DISK".bold(),
        "USD/HR".bold(),
        "REGION".bold(),
        "REL".bold(),
    );
    for o in offers {
        println!(
            "{:<10} {:<20} {:>5} {:>8} {:>6} {:>10.4} {:<6} {:>6}",
            o.id,
            o.gpu_name,
            o.num_gpus,
            o.vram_gb,
            o.disk_gb,
            o.price_per_hour_usd,
            o.region.as_deref().unwrap_or("-"),
            o.reliability
                .map(|r| format!("{r:.3}"))
                .unwrap_or_else(|| "-".into()),
        );
    }
}
