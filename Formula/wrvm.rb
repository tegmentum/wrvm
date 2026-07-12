class Wrvm < Formula
  desc "wrvm — WAMR (WebAssembly Micro Runtime) version manager"
  homepage "https://github.com/tegmentum/wrvm"
  version "0.1.0"
  license "Apache-2.0"

  # Fill in per-platform sha256 values from the published release assets.
  on_macos do
    if Hardware::CPU.intel?
      # v0.1.0 doesn't ship an Intel macOS binary because GitHub Actions
      # macos-13 (Intel) runners are backlogged for hours. Build from source:
      #   git clone https://github.com/tegmentum/wrvm && cd wrvm && cargo build --release
      odie "wrvm v#{version} has no prebuilt Intel macOS binary yet — build from source with `cargo build --release`, or install via `curl | sh` which falls back to `cargo build`."
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-macos"
      sha256 "f530c9093a30fab2e1571a534c1c8dbdafa3622bb1289cab6ba37d685f117d3f"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-x86_64-linux"
      sha256 "1ebf02c881544c1207b46ce3856317132a3e5fa18d2ff8eb60eb53181c5bf4b9"
    else
      url "https://github.com/tegmentum/wrvm/releases/download/v#{version}/wrvm-aarch64-linux"
      sha256 "40b4392657bdeea834325a822f5b6d1e3d32e074101fc90cef485f81c58c1827"
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
