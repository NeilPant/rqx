use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree, settings};

const REPOSITORY: &str = "jadhamwi/rqx";

#[derive(Default)]
struct Rqx {
    /// Kept so a worktree that opens later reuses the binary the first one downloaded
    /// rather than fetching it again.
    binary: Option<String>,
}

impl Rqx {
    fn resolve(&mut self, id: &LanguageServerId, worktree: &Worktree) -> Result<String> {
        // Whatever the developer installed on purpose wins: a managed copy that quietly
        // shadowed it would let the editor and CI run different versions.
        if let Some(path) = worktree.which("rqx") {
            return Ok(path);
        }
        if let Some(path) = self.binary.clone() {
            return Ok(path);
        }

        zed::set_language_server_installation_status(
            id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            REPOSITORY,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;
        let version = release.version.trim_start_matches('v');
        let (platform, architecture) = zed::current_platform();
        let target = match (platform, architecture) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => "aarch64-apple-darwin",
            (zed::Os::Mac, zed::Architecture::X8664) => "x86_64-apple-darwin",
            (zed::Os::Linux, zed::Architecture::Aarch64) => "aarch64-unknown-linux-gnu",
            (zed::Os::Linux, zed::Architecture::X8664) => "x86_64-unknown-linux-gnu",
            (zed::Os::Windows, zed::Architecture::X8664) => "x86_64-pc-windows-msvc",
            _ => return Err("no published rqx build for this platform".into()),
        };

        let asset_name = format!("rqx-{version}-{target}.tar.gz");
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("release {version} has no {asset_name}"))?;

        let directory = format!("rqx-{version}");
        let binary = format!(
            "{directory}/rqx{}",
            if platform == zed::Os::Windows { ".exe" } else { "" }
        );
        if !std::fs::metadata(&binary).is_ok_and(|entry| entry.is_file()) {
            zed::set_language_server_installation_status(
                id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(
                &asset.download_url,
                &directory,
                zed::DownloadedFileType::GzipTar,
            )?;
            zed::make_file_executable(&binary)?;
            // Anything left from an older version is dead weight in the extension's
            // working directory, and Zed never cleans it up for us.
            for entry in std::fs::read_dir(".").map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                if entry.file_name().to_str() != Some(&directory) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }

        self.binary = Some(binary.clone());
        Ok(binary)
    }
}

impl zed::Extension for Rqx {
    fn new() -> Self {
        Self::default()
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        Ok(Command {
            command: self.resolve(id, worktree)?,
            args: vec!["lsp".to_owned()],
            env: Default::default(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(settings::LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options))
    }
}

zed::register_extension!(Rqx);
