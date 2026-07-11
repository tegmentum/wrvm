class Wrvm < Formula
  desc "wrvm — WAMR (WebAssembly Micro Runtime) version manager"
  homepage "https://github.com/tegmentum/wrvm"
  version "0.1.0"
  license "Apache-2.0"

  # Fill in per-platform sha256 values from the published release assets.
  on_macos do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-macos"
      sha256 "FILL_IN_SHA256_MACOS_X86_64"
    else
      # WAMR upstream ships x86_64 binaries only; wrvm itself is still available.
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-macos"
      sha256 "FILL_IN_SHA256_MACOS_AARCH64"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-linux"
      sha256 "FILL_IN_SHA256_LINUX_X86_64"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-linux"
      sha256 "FILL_IN_SHA256_LINUX_AARCH64"
    end
  end

  def install
    bin.install Dir.glob("wrvm-*").first => "wrvm"
    generate_completions_from_executable(bin/"wrvm", "completions")
  end

  def caveats
    <<~EOS
      Enable per-shell `wrvm use` and the pass-through `iwasm` shim:
          wrvm shell-init >> ~/.zshrc     # or your shell's rc
      Then restart your shell.

      NOTE: WAMR upstream publishes x86_64 binaries only. On aarch64
      (Apple Silicon, ARM Linux), `wrvm install` will refuse with a pointer
      at the upstream gap; wrvm itself installs regardless.
    EOS
  end

  test do
    assert_match "wrvm #{version}", shell_output("#{bin}/wrvm --version")
  end
end
