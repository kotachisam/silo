use crate::providers::Offer;
use owo_colors::OwoColorize;

pub fn render_offers(offers: &[Offer]) {
    if offers.is_empty() {
        println!("(no offers matched)");
        return;
    }
    render_perf_block(offers);
    println!();
    render_infra_block(offers);
    println!();
    render_country_block(offers);
}

fn render_perf_block(offers: &[Offer]) {
    println!(
        "{:>3}  {:<8}  {:<4}  {:<2}  {:<15}  {:>4}  {:>7}  {:>5}  {:>5}  {:>5}  {:>4}  {:>6}  {:>5}  {:>6}",
        "#".bold(),
        "ID".bold(),
        "CUDA".bold(),
        "N".bold(),
        "Model".bold(),
        "PCIE".bold(),
        "cpu_ghz".bold(),
        "vCPUs".bold(),
        "RAM".bold(),
        "VRAM".bold(),
        "Disk".bold(),
        "$/hr".bold(),
        "DLP".bold(),
        "DLP/$".bold(),
    );
    for (i, o) in offers.iter().enumerate() {
        println!(
            "{:>3}  {:<8}  {:<4}  {:<2}  {:<15}  {:>4}  {:>7}  {:>5}  {:>5}  {:>5}  {:>4}  {:>6.4}  {:>5}  {:>6}",
            i + 1,
            truncate(&o.id, 8),
            o.cuda.as_deref().unwrap_or("-"),
            format!("{}x", o.num_gpus),
            truncate(&o.gpu_name.replace(' ', "_"), 15),
            o.pcie_bw.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.cpu_ghz.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.vcpus.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.ram_gb.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            format!("{:.1}", o.vram_gb),
            o.disk_gb,
            o.price_per_hour_usd,
            o.dlp.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.dlp_per_dollar.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into()),
        );
    }
}

fn render_infra_block(offers: &[Offer]) {
    println!(
        "{:>3}  {:>5}  {:<10}  {:>7}  {:>8}  {:>4}  {:>8}  {:>7}  {:<10}  {:>7}  {:>5}",
        "#".bold(),
        "score".bold(),
        "NV Driver".bold(),
        "Net_up".bold(),
        "Net_down".bold(),
        "R".bold(),
        "Max_Days".bold(),
        "mach_id".bold(),
        "status".bold(),
        "host_id".bold(),
        "ports".bold(),
    );
    for (i, o) in offers.iter().enumerate() {
        println!(
            "{:>3}  {:>5}  {:<10}  {:>7}  {:>8}  {:>4}  {:>8}  {:>7}  {:<10}  {:>7}  {:>5}",
            i + 1,
            o.score.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            truncate(o.driver.as_deref().unwrap_or("-"), 10),
            o.net_up_mbps.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.net_down_mbps.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.reliability.map(|v| format!("{:.1}", v * 100.0)).unwrap_or_else(|| "-".into()),
            o.max_days.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into()),
            o.machine_id.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            truncate(o.status.as_deref().unwrap_or("-"), 10),
            o.host_id.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
            o.ports.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        );
    }
}

fn render_country_block(offers: &[Offer]) {
    println!("{:>3}  {}", "#".bold(), "country".bold());
    for (i, o) in offers.iter().enumerate() {
        let country = o
            .country
            .as_deref()
            .unwrap_or("-")
            .replace(", ", ",_");
        println!("{:>3}  {country}", i + 1);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}
