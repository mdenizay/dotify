# Template for Casks/dotify.rb in mdenizay/homebrew-tap; tools/release.sh fills
# in VERSION and SHA256 and strips these comment lines.
cask "dotify" do
  version "VERSION"
  sha256 "SHA256"

  url "https://github.com/mdenizay/dotify/releases/download/v#{version}/Dotify-#{version}.zip"
  name "Dotify"
  desc "Pixel art editor and sprite animation studio for indie game developers"
  homepage "https://github.com/mdenizay/dotify"

  auto_updates true
  depends_on macos: :ventura
  depends_on arch: :arm64

  app "Dotify.app"

  uninstall quit: "app.dotify.desktop"

  zap trash: [
    "~/Library/Application Support/app.dotify.desktop",
    "~/Library/Caches/app.dotify.desktop",
    "~/Library/Preferences/app.dotify.desktop.plist",
    "~/Library/WebKit/app.dotify.desktop",
  ]
end
