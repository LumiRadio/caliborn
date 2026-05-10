# Minimal dev liquidsoap config.
# Exposes a Unix socket so caliborn can talk to it, and defines a
# playlist source named "playlist" (matches caliborn's default
# CALIBORN__LIQUIDSOAP_PLAYLIST_SOURCE).

settings.init.allow_root.set(true)

settings.server.socket.set(true)
settings.server.socket.path.set("/sock/liquidsoap.sock")
settings.server.socket.permissions.set(0o666)

settings.log.stdout.set(true)
settings.log.level.set(3)

# Empty playlist file is fine; caliborn writes to it via its `playlist` cmd.
playlist_src = playlist(
  id="playlist",
  reload_mode="manual",
  "/music/playlist.m3u"
)

# Fallible-safe dummy output so liquidsoap stays alive even if playlist empty.
output.dummy(fallible=true, playlist_src)
