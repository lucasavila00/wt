# Make shell playback pane selection durable

Outside Live, the screen tracker associates a world stream with its sole active
Codex session without first selecting or validating the displayed pane. Even
after Live validates and selects the pane, tmux selection is shared, so another
attached client can change it without replacing WT's connection. WT can then
mistake another pane's output for the associated Codex session.

Give the WT playback client independent pane ownership, or continuously validate
which pane that client displays. The solution must identify the specific tmux
client, define how focus commands address it, detect invalidation, and test a
second client changing the shared session. Do not add screen capture, OCR, or a
second terminal stream.
