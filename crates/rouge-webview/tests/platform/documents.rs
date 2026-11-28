pub(super) const ANNOUNCES: &str = "<script>parent.postMessage('loaded', '*')</script>";
pub(super) const ECHOES: &str = "<script>addEventListener('message', (event) => {\
                      if (!event.source) parent.postMessage('echo ' + event.data, '*');\
                      });parent.postMessage('ready', '*')</script>";
pub(super) const AWKWARD: &str = "she said \"</script>\" \\ and\nleft\u{2028}";

pub(super) fn echo() -> String {
    format!("echo {AWKWARD}")
}
