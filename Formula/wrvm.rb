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

  # Wire the wrvm shell integration into the user's login-shell rc file so
  # `iwasm` on PATH routes through wrvm and `wrvm use` works immediately.
  # Matches what `install.sh` does; idempotent via a `# wrvm-managed:env`
  # tag so re-runs are no-ops and uninstall is one grep away.
  #
  # (Homebrew's style guide discourages this for core formulas. For this
  # third-party tap the "shell-init is part of the install" UX wins.)
  def post_install
    home = ENV["HOME"]
    return if home.nil? || home.empty?

    shell_base = File.basename(ENV["SHELL"] || "")
    rc, line = case shell_base
               when "zsh"
                 [File.join(ENV["ZDOTDIR"] || home, ".zshrc"),
                  'eval "$(wrvm shell-init)" # wrvm-managed:env']
               when "bash"
                 [File.join(home, ".bashrc"),
                  'eval "$(wrvm shell-init)" # wrvm-managed:env']
               when "fish"
                 [File.join(home, ".config/fish/config.fish"),
                  'wrvm shell-init | source # wrvm-managed:env']
               else
                 [File.join(home, ".profile"),
                  'eval "$(wrvm shell-init)" # wrvm-managed:env']
               end

    FileUtils.mkdir_p(File.dirname(rc))
    existing = File.exist?(rc) ? File.read(rc) : ""

    if existing.include?("# wrvm-managed:env")
      ohai "wrvm shell integration already present in #{rc}"
      return
    end

    File.open(rc, "a") do |f|
      f.puts if !existing.empty? && !existing.end_with?("\n")
      f.puts line
    end
    ohai "Added wrvm shell integration to #{rc}"
  end

  def caveats
    <<~EOS
      wrvm shell integration was added to your shell rc (tagged `# wrvm-managed`).
      Restart your shell — or run `eval "$(wrvm shell-init)"` — to activate it now.

      Remove the integration with:
          grep -v '# wrvm-managed' ~/.zshrc > ~/.zshrc.tmp && mv ~/.zshrc.tmp ~/.zshrc

      NOTE: WAMR upstream publishes x86_64 binaries only. On aarch64 hosts
      (Apple Silicon, ARM Linux), `wrvm install` resolves runtime downloads
      from an in-repo mirror channel — see the README for details.
    EOS
  end

  test do
    assert_match "wrvm #{version}", shell_output("#{bin}/wrvm --version")
  end
end
