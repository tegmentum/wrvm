class Wrvm < Formula
  desc "wrvm — WAMR (WebAssembly Micro Runtime) version manager"
  homepage "https://github.com/tegmentum/wrvm"
  version "0.1.2"
  license "Apache-2.0"

  # Fill in per-platform sha256 values from the published release assets.
  on_macos do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-macos"
      sha256 "ae3cc128d847827a4dc6686023d14905c0b734492fddb028d8da659069eeca08"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-macos"
      sha256 "63976ec763a6c8c0404e5a10934585a579d9a585ef74178c38de4fa8a480c21a"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-linux"
      sha256 "c5e86f0a3eaf67ee4dec9a6a0191db32488e153717d2646c01e4bc5b22491296"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-linux"
      sha256 "c14be1c52da1beee853b9504ebd8e7372840bd9b13d3b20b90c49e3ba74e1bd3"
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
      To finish setup — wire `iwasm`/`wamrc` on PATH through wrvm's shims
      and enable per-shell `wrvm use` — run once:

          wrvm setup

      Then restart your shell (or run `eval "$(wrvm shell-init)"` to
      activate it in the current shell without restarting).

      `wrvm setup` is idempotent and tags its line with `# wrvm-managed`
      so uninstalling the integration is a one-liner:

          grep -v '# wrvm-managed' ~/.zshrc > ~/.zshrc.tmp && mv ~/.zshrc.tmp ~/.zshrc

      Homebrew sandboxes formula install steps and cannot safely modify
      files under $HOME, which is why `wrvm setup` runs from wrvm itself
      rather than as part of `brew install`.

      NOTE: WAMR upstream publishes x86_64 binaries only. On aarch64 hosts
      (Apple Silicon, ARM Linux), `wrvm install` resolves runtime downloads
      from an in-repo mirror channel — see the README for details.
    EOS
  end

  test do
    assert_match "wrvm #{version}", shell_output("#{bin}/wrvm --version")
  end
end
