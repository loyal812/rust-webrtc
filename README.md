# WebRTC video/audio stream

A simple WebRTC streaming server. It streams video and audio from a file to a browser client.

## Try it out!
1. Install [Rust](https://rustup.rs/)
2. Serve WebRTC demo client (for example with [serve-directory](https://crates.io/crates/serve-directory))
```bash
$ cargo install serve-directory
$ serve-directory -p 8080 &
```
3. Open `http://localhost:8080/webrtc.html` in your browser
4. Replace contents of `browser_session_description.txt` file with a session description from the demo client
5. Run the app with the session description from the demo client:
```bash
$ cat browser_session_description.txt | cargo run --release
```
6. It should set your buffer with a server session description. Copy and paste it to the demo client
7. Click "Start session" on the demo client