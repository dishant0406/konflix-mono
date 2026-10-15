# The Homebrew cask for the macOS app. The "Release desktop" workflow rewrites `version` and
# `sha256` after it publishes a release. Install with:
#   brew tap dishant0406/akidev https://github.com/dishant0406/konflix-mono
#   brew install --cask akidev
cask "akidev" do
  version "0.1.3"
  sha256 "4a1c9e82e549bbfcde2b02f4ce4727de7234656fbf701d7a4329b4e4ec32e96d"

  url "https://github.com/dishant0406/konflix-mono/releases/download/desktop-mac-v#{version}/Akidev-#{version}-arm64.dmg"
  name "Akidev"
  desc "Browse and stream anime, series and films"
  homepage "https://akidev.live"

  livecheck do
    url "https://github.com/dishant0406/konflix-mono/releases"
    regex(/desktop-mac-v(\d+(?:\.\d+)+)/i)
    strategy :page_match
  end

  # The app updates itself, so brew does not need to.
  auto_updates true
  depends_on arch: :arm64
  depends_on macos: ">= :monterey"

  app "Akidev.app"

  zap trash: [
    "~/Library/Application Support/live.akidev.Akidev",
    "~/Library/Caches/live.akidev.Akidev",
  ]
end
