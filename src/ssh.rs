use anyhow::{Context, Result};
use std::process::Command;

fn apply_hardening(c: &mut Command) {
    for opt in [
        "BatchMode=yes",
        "StrictHostKeyChecking=accept-new",
        "ConnectTimeout=15",
        "ServerAliveInterval=5",
        "ServerAliveCountMax=3",
    ] {
        c.arg("-o").arg(opt);
    }
}

pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl SshTarget {
    pub fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            user: "root".into(),
        }
    }

    pub fn build_ssh(&self, remote_command: &[String]) -> Command {
        let mut c = Command::new("ssh");
        apply_hardening(&mut c);
        c.arg("-p").arg(self.port.to_string());
        c.arg(format!("{}@{}", self.user, self.host));
        for piece in remote_command {
            c.arg(piece);
        }
        c
    }

    pub fn build_tunnel(&self, local_port: u16, remote_port: u16) -> Command {
        let mut c = Command::new("ssh");
        apply_hardening(&mut c);
        c.arg("-p").arg(self.port.to_string());
        c.arg("-L")
            .arg(format!("{local_port}:localhost:{remote_port}"));
        c.arg("-N");
        c.arg(format!("{}@{}", self.user, self.host));
        c
    }

    pub fn run_ssh(&self, remote_command: &[String]) -> Result<()> {
        let status = self
            .build_ssh(remote_command)
            .status()
            .context("spawning ssh")?;
        if !status.success() {
            anyhow::bail!("ssh exited with {status}");
        }
        Ok(())
    }

    pub fn run_tunnel(&self, local_port: u16, remote_port: u16) -> Result<()> {
        let status = self
            .build_tunnel(local_port, remote_port)
            .status()
            .context("spawning ssh tunnel")?;
        if !status.success() {
            anyhow::bail!("ssh tunnel exited with {status}");
        }
        Ok(())
    }

    pub fn run_ssh_to_file(
        &self,
        remote_command: &[String],
        local_path: &std::path::Path,
    ) -> Result<()> {
        use std::process::Stdio;
        let file = std::fs::File::create(local_path)
            .with_context(|| format!("creating {}", local_path.display()))?;
        let status = self
            .build_ssh(remote_command)
            .stdout(Stdio::from(file))
            .status()
            .context("spawning ssh")?;
        if !status.success() {
            anyhow::bail!("ssh exited with {status}");
        }
        Ok(())
    }

    pub fn run_ssh_with_stdin(
        &self,
        remote_command: &[String],
        stdin_bytes: &[u8],
    ) -> Result<String> {
        use std::io::Write;
        use std::process::Stdio;

        let mut child = self
            .build_ssh(remote_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawning ssh")?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_bytes)
                .context("writing to ssh stdin")?;
        }

        let output = child.wait_with_output().context("waiting for ssh")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ssh exited with {}: {}", output.status, stderr.trim());
        }
        String::from_utf8(output.stdout).context("ssh stdout not valid UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(c: &Command) -> Vec<String> {
        std::iter::once(c.get_program())
            .chain(c.get_args())
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    fn with_opts(rest: &[&str]) -> Vec<String> {
        let mut v = vec!["ssh".to_string()];
        for opt in [
            "BatchMode=yes",
            "StrictHostKeyChecking=accept-new",
            "ConnectTimeout=15",
            "ServerAliveInterval=5",
            "ServerAliveCountMax=3",
        ] {
            v.push("-o".to_string());
            v.push(opt.to_string());
        }
        v.extend(rest.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn build_ssh_hardening_options_present() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_ssh(&[]));
        assert!(argv.windows(2).any(|w| w == ["-o", "BatchMode=yes"]));
        assert!(
            argv.windows(2)
                .any(|w| w == ["-o", "StrictHostKeyChecking=accept-new"])
        );
        assert!(
            argv.windows(2)
                .any(|w| w == ["-o", "ServerAliveInterval=5"])
        );
    }

    #[test]
    fn build_ssh_basic_form() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_ssh(&[]));
        assert_eq!(argv, with_opts(&["-p", "12345", "root@ssh4.vast.ai"]));
    }

    #[test]
    fn build_ssh_appends_remote_command() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_ssh(&["ollama".into(), "list".into()]));
        assert_eq!(
            argv,
            with_opts(&["-p", "12345", "root@ssh4.vast.ai", "ollama", "list"])
        );
    }

    #[test]
    fn build_tunnel_for_ollama() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_tunnel(11434, 11434));
        assert_eq!(
            argv,
            with_opts(&[
                "-p",
                "12345",
                "-L",
                "11434:localhost:11434",
                "-N",
                "root@ssh4.vast.ai"
            ])
        );
    }
}
