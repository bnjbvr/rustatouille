# rustatouille

rustatouille is an incident page system, entirely written in Rust!

# [Spec](https://annuel2.framapad.org/p/statut-rs)

## How to run

- Make sure that you've installed [rust](https://rustup.rs/), and that you're using the latest
  version.
- Tweak environment variables (see the `.env` for an explanation of possible values) as you need.
- Start `cargo run`.

Then, a line "listening on ..." will let you know on which interface/port the Web app is listening.

By default, there's a (bad) static server stub, for quick testing; it's recommended to start an
actual static HTTP server in the cache directory (e.g. using python3, with `python -m http.server`).

The main admin page is available at `/admin` (note: no leading slash in the embedded static
server).
