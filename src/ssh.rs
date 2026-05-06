use anyhow::{Context, Result};
use std::process::Command;

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
        c.arg("-p").arg(self.port.to_string());
        c.arg(format!("{}@{}", self.user, self.host));
        for piece in remote_command {
            c.arg(piece);
        }
        c
    }

    pub fn build_tunnel(&self, local_port: u16, remote_port: u16) -> Command {
        let mut c = Command::new("ssh");
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

    #[test]
    fn build_ssh_basic_form() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_ssh(&[]));
        assert_eq!(argv, vec!["ssh", "-p", "12345", "root@ssh4.vast.ai"]);
    }

    #[test]
    fn build_ssh_appends_remote_command() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_ssh(&["ollama".into(), "list".into()]));
        assert_eq!(
            argv,
            vec!["ssh", "-p", "12345", "root@ssh4.vast.ai", "ollama", "list"]
        );
    }

    #[test]
    fn build_tunnel_for_ollama() {
        let t = SshTarget::new("ssh4.vast.ai".into(), 12345);
        let argv = argv(&t.build_tunnel(11434, 11434));
        assert_eq!(
            argv,
            vec![
                "ssh",
                "-p",
                "12345",
                "-L",
                "11434:localhost:11434",
                "-N",
                "root@ssh4.vast.ai"
            ]
        );
    }
}
