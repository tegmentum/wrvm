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
