CloudDeck for Linux (x86_64)
============================

Install
-------
Extract the archive anywhere and run the app:

    tar -xzf CloudDeck-<version>-linux-x86_64.tar.gz
    ./clouddeck

Optional desktop integration (the launcher runs "clouddeck" from PATH):

    mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons
    cp clouddeck ~/.local/bin/
    cp clouddeck.desktop ~/.local/share/applications/
    cp -r share/icons/hicolor ~/.local/share/icons/

Runtime requirements
--------------------
- A Vulkan driver for your GPU
- libxkbcommon
- fontconfig
- A Wayland or X11 session

Updates
-------
Choose "Check for Updates..." in the app menu to download and install a newer
release. While the repository is private, updates need GitHub access: run
"gh auth login" once, or set the CLOUDDECK_UPDATE_TOKEN environment variable.
