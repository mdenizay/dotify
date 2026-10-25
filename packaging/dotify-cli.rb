# Template for Formula/dotify-cli.rb in mdenizay/homebrew-tap; tools/release.sh
# fills in VERSION and SHA256 and strips these comment lines.
class DotifyCli < Formula
  desc "Headless Dotify project editing, export and MCP server"
  homepage "https://github.com/mdenizay/dotify"
  url "https://github.com/mdenizay/dotify/releases/download/vVERSION/dotify-cli-VERSION-macos-arm64.tar.gz"
  sha256 "SHA256"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on arch: :arm64
  depends_on :macos

  def install
    bin.install "dotify", "dotify-mcp"
  end

  test do
    system bin/"dotify", "new", testpath/"t.dotify", "--width", "4", "--height", "3"
    assert_match '"height": 3', shell_output("#{bin}/dotify info #{testpath}/t.dotify")
    assert_match version.to_s, shell_output("#{bin}/dotify-mcp --version")
    assert_equal 64, shell_output("#{bin}/dotify-mcp token").strip.length
  end
end
