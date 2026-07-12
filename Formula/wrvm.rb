class Wrvm < Formula
  desc "wrvm — WAMR (WebAssembly Micro Runtime) version manager"
  homepage "https://github.com/tegmentum/wrvm"
  version "0.1.1"
  license "Apache-2.0"

  # Fill in per-platform sha256 values from the published release assets.
  on_macos do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-macos"
      sha256 "1dafacf5c256260c5a20e0eb83856f628e82a48148b84a03a0e184da7212a4af"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-macos"
      sha256 "a3dd52aaf8083888b8e62cef359237b411eb4fd323b061b949ee92cf195eb051"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-linux"
      sha256 "fbf89a41272405bbab25a3121947816783afb81e0567d212156b0afd299d079d"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-linux"
      sha256 "405a18a9e658db8356c97ea7b3f084faea0349fe5f1b4eb50b268fd04f5ae0b2"
    end
  end

  def install
    # GitHub release assets are downloaded as raw files (not tarballs), so
    # they arrive without the executable bit. Restore it before install so
    # `generate_completions_from_executable` can actually invoke the binary.
    src = Dir.glob("wrvm-*").first
    File.chmod(0755, src)
    bin.install src => "wrvm"
    generate_completions_from_executable(bin/"wrvm", "completions")
  end

  # Ship a shell-integration snippet at a stable Cellar path so the caveat
  # gives users a single unchanging line to source. Homebrew sandboxes
  # post_install and redirects $HOME, so formulas can't safely wire up
  # user rc files themselves — the `curl | sh` installer does that. This
  # is the Homebrew-idiomatic compromise (nvm, zoxide, direnv all do the
  # same: install the machinery, ask the user to add one line to their rc).
  def post_install
    (share/"wrvm").mkpath
    (share/"wrvm/wrvm.sh").write <<~SH
      # wrvm shell integration (source from bash/zsh rc).
      # Regenerates on every shell start so wrvm updates are picked up.
      command -v wrvm >/dev/null && eval "$(wrvm shell-init)"
    SH
    (share/"wrvm/wrvm.fish").write <<~FISH
      # wrvm shell integration for fish.
      command -v wrvm >/dev/null; and wrvm shell-init | source
    FISH
  end

  def caveats
    <<~EOS
      To enable per-shell `wrvm use` and route `iwasm`/`wamrc` on PATH
      through wrvm's shims, add one line to your shell rc:

          # bash / zsh:
          echo 'source "$(brew --prefix wrvm)/share/wrvm/wrvm.sh"' >> ~/.zshrc

          # fish:
          echo 'source (brew --prefix wrvm)/share/wrvm/wrvm.fish' \\
              >> ~/.config/fish/config.fish

      Then restart your shell (or run `eval "$(wrvm shell-init)"` to
      activate it in the current shell without restarting).

      Homebrew sandboxes formula install steps and cannot safely modify
      files under $HOME; the `curl | sh` installer wires this up
      automatically.

      NOTE: WAMR upstream publishes x86_64 binaries only. On aarch64 hosts
      (Apple Silicon, ARM Linux), `wrvm install` resolves runtime downloads
      from an in-repo mirror channel — see the README for details.
    EOS
  end

  test do
    assert_match "wrvm #{version}", shell_output("#{bin}/wrvm --version")
  end
end
