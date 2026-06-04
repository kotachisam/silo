use crate::models::HfModel;
use crate::providers::{Offer, ProviderExtra, VastExtra};
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

fn vast_extra(o: &Offer) -> Option<&VastExtra> {
    match &o.extra {
        ProviderExtra::Vast(v) => Some(v),
    }
}

pub fn render_offers(offers: &[Offer]) {
    if offers.is_empty() {
        println!("(no offers matched)");
        return;
    }
    render_core_block(offers);
    if offers.iter().any(|o| vast_extra(o).is_some()) {
        println!();
        render_vast_block(offers);
    }
}

fn render_core_block(offers: &[Offer]) {
    println!(
        "{:>3}  {:<8}  {:<8}  {:<2}  {:<15}  {:>7}  {:>7}  {:>6}  {:>4}  {:>5}  {:>6}  {:>3}  {:>7}  {:>8}  {:<12}",
        "#".bold(),
        "Provider".bold(),
        "ID".bold(),
        "N".bold(),
        "Model".bold(),
        "VRAM/GB".bold(),
        "Disk/GB".bold(),
        "$/hr".bold(),
        "R%".bold(),
        "vCPUs".bold(),
        "RAM/GB".bold(),
        "GHz".bold(),
        "Net_up".bold(),
        "Net_down".bold(),
        "region".bold(),
    );
    for (i, o) in offers.iter().enumerate() {
        println!(
            "{:>3}  {:<8}  {:<8}  {:<2}  {:<15}  {:>7}  {:>7}  {:>6.4}  {:>4}  {:>5}  {:>6}  {:>3}  {:>7}  {:>8}  {:<12}",
            i + 1,
            o.provider.as_str(),
            truncate(&o.id, 8),
            format!("{}x", o.num_gpus),
            truncate(&o.gpu_raw.replace(' ', "_"), 15),
            format!("{:.1}", o.vram_gb),
            o.disk_gb,
            o.price_per_hour_usd,
            o.reliability
                .map(|v| format!("{:.1}", v * 100.0))
                .unwrap_or_else(|| "-".into()),
            o.vcpus
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.ram_gb
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.cpu_ghz
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.net_up_mbps
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            o.net_down_mbps
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            truncate(o.region.as_deref().unwrap_or("-"), 12).replace(", ", ",_"),
        );
    }
}

fn render_vast_block(offers: &[Offer]) {
    println!(
        "{:>3}  {:<4}  {:>4}  {:>5}  {:>6}  {:>5}  {:<10}  {:>8}  {:>7}  {:<10}  {:>7}  {:>5}  {}",
        "#".bold(),
        "CUDA".bold(),
        "PCIE".bold(),
        "DLP".bold(),
        "DLP/$".bold(),
        "score".bold(),
        "NV Driver".bold(),
        "Max_Days".bold(),
        "mach_id".bold(),
        "status".bold(),
        "host_id".bold(),
        "ports".bold(),
        "country".bold(),
    );
    let dash = || "-".to_string();
    for (i, o) in offers.iter().enumerate() {
        let v = vast_extra(o);
        let country = v
            .and_then(|x| x.country.as_deref())
            .unwrap_or("-")
            .replace(", ", ",_");
        println!(
            "{:>3}  {:<4}  {:>4}  {:>5}  {:>6}  {:>5}  {:<10}  {:>8}  {:>7}  {:<10}  {:>7}  {:>5}  {}",
            i + 1,
            v.and_then(|x| x.cuda.as_deref()).unwrap_or("-"),
            v.and_then(|x| x.pcie_bw)
                .map(|x| format!("{x:.1}"))
                .unwrap_or_else(dash),
            v.and_then(|x| x.dlp)
                .map(|x| format!("{x:.1}"))
                .unwrap_or_else(dash),
            v.and_then(|x| x.dlp_per_dollar)
                .map(|x| format!("{x:.2}"))
                .unwrap_or_else(dash),
            v.and_then(|x| x.score)
                .map(|x| format!("{x:.1}"))
                .unwrap_or_else(dash),
            truncate(v.and_then(|x| x.driver.as_deref()).unwrap_or("-"), 10),
            v.and_then(|x| x.max_days)
                .map(humanize_days)
                .unwrap_or_else(dash),
            v.and_then(|x| x.machine_id)
                .map(|x| x.to_string())
                .unwrap_or_else(dash),
            truncate(v.and_then(|x| x.status.as_deref()).unwrap_or("-"), 10),
            v.and_then(|x| x.host_id)
                .map(|x| x.to_string())
                .unwrap_or_else(dash),
            v.and_then(|x| x.ports)
                .map(|x| x.to_string())
                .unwrap_or_else(dash),
            country,
        );
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
