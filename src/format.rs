use crate::models::HfModel;
use crate::providers::Offer;
use owo_colors::OwoColorize;

pub fn render_models(models: &[HfModel], filtered_out: usize) {
    if models.is_empty() {
        println!("(no models matched)");
        if filtered_out > 0 {
            println!(
                "({filtered_out} excluded by --min/--max; drop or widen the filter to see them)"
            );
        }
        return;
    }
    println!(
        "{:>3}  {:<55}  {:>7}  {:>10}  {:<10}",
        "#".bold(),
        "Model".bold(),
        "Params".bold(),
        "Downloads".bold(),
        "Updated".bold(),
    );
    for (i, m) in models.iter().enumerate() {
        println!(
            "{:>3}  {:<55}  {:>7}  {:>10}  {:<10}",
            i + 1,
            truncate(&m.id, 55),
            m.params_billions()
                .map(format_params)
                .unwrap_or_else(|| "?".into()),
            humanize_downloads(m.downloads),
            short_date(m.last_modified.as_deref()),
        );
    }
    if filtered_out > 0 {
        let total = models.len() + filtered_out;
        println!();
        println!(
            "({filtered_out} of {total} in this batch excluded by --min/--max — pass `--limit 50` or higher to widen the trending pool)"
        );
    }
}

fn format_params(p: f32) -> String {
    if p >= 1000.0 {
        format!("{:.1}T", p / 1000.0)
    } else if p >= 100.0 {
        format!("{p:.0}B")
    } else if p >= 10.0 {
        format!("{p:.1}B")
    } else {
        format!("{p:.2}B")
    }
}

fn humanize_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

fn short_date(s: Option<&str>) -> String {
    s.and_then(|d| d.split('T').next())
        .unwrap_or("?")
        .to_string()
}

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
        "{:>3}  {:<8}  {:<4}  {:<2}  {:<15}  {:>4}  {:>3}  {:>5}  {:>6}  {:>7}  {:>7}  {:>6}  {:>5}  {:>6}",
        "#".bold(),
        "ID".bold(),
        "CUDA".bold(),
        "N".bold(),
        "Model".bold(),
        "PCIE".bold(),
        "GHz".bold(),
        "vCPUs".bold(),
        "RAM/GB".bold(),
        "VRAM/GB".bold(),
        "Disk/GB".bold(),
        "$/hr".bold(),
        "DLP".bold(),
        "DLP/$".bold(),
    );
    for (i, o) in offers.iter().enumerate() {
        println!(
            "{:>3}  {:<8}  {:<4}  {:<2}  {:<15}  {:>4}  {:>3}  {:>5}  {:>6}  {:>7}  {:>7}  {:>6.4}  {:>5}  {:>6}",
            i + 1,
            truncate(&o.id, 8),
            o.cuda.as_deref().unwrap_or("-"),
            format!("{}x", o.num_gpus),
            truncate(&o.gpu_name.replace(' ', "_"), 15),
            o.pcie_bw
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.cpu_ghz
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.vcpus
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.ram_gb
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            format!("{:.1}", o.vram_gb),
            o.disk_gb,
            o.price_per_hour_usd,
            o.dlp
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.dlp_per_dollar
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".into()),
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
        "R%".bold(),
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
            o.score
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            truncate(o.driver.as_deref().unwrap_or("-"), 10),
            o.net_up_mbps
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.net_down_mbps
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.reliability
                .map(|v| format!("{:.1}", v * 100.0))
                .unwrap_or_else(|| "-".into()),
            o.max_days.map(humanize_days).unwrap_or_else(|| "-".into()),
            o.machine_id
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            truncate(o.status.as_deref().unwrap_or("-"), 10),
            o.host_id
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            o.ports.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
        );
    }
}

fn render_country_block(offers: &[Offer]) {
    println!("{:>3}  {}", "#".bold(), "country".bold());
    for (i, o) in offers.iter().enumerate() {
        let country = o.country.as_deref().unwrap_or("-").replace(", ", ",_");
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

fn humanize_days(days: f32) -> String {
    let d = days as i32;
    if d < 1 {
        format!("{:.1}h", days * 24.0)
    } else if d < 31 {
        format!("{d}d")
    } else if d < 365 {
        let months = d / 30;
        let extra = d % 30;
        if extra == 0 {
            format!("{months}mo")
        } else {
            format!("{months}mo {extra}d")
        }
    } else {
        let years = d / 365;
        let extra = d % 365;
        if extra == 0 {
            format!("{years}y")
        } else {
            format!("{years}y {extra}d")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_handles_sub_day() {
        assert_eq!(humanize_days(0.5), "12.0h");
    }

    #[test]
    fn humanize_handles_days() {
        assert_eq!(humanize_days(15.7), "15d");
    }

    #[test]
    fn humanize_handles_months() {
        assert_eq!(humanize_days(95.0), "3mo 5d");
        assert_eq!(humanize_days(60.0), "2mo");
    }

    #[test]
    fn humanize_handles_years() {
        assert_eq!(humanize_days(605.3), "1y 240d");
        assert_eq!(humanize_days(730.0), "2y");
    }
}
