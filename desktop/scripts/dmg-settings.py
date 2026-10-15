# Settings for dmgbuild (see dmg-macos.sh): how the disk image's window looks. The window is the
# 660x400 background; the app and the Applications shortcut sit on its left and right, with the
# arrow between them drawn into the picture.
import os

app = os.environ["DMG_APP"]
background = os.environ["DMG_BACKGROUND"]

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.environ.get("DMG_VOLUME_ICON") or None

background = background
# 400 points of picture under a 32 point title bar.
window_rect = ((200, 140), (660, 432))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
arrange_by = None
icon_size = 112
text_size = 13
icon_locations = {
    os.path.basename(app): (180, 190),
    "Applications": (480, 190),
}
