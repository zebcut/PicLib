# Security policy

Only the latest release (and `main`) gets security fixes.

**Please don't open a public issue for a vulnerability.** Use GitHub's private
reporting instead: **Security** tab → **Report a vulnerability**. Include the
version, steps to reproduce and the impact if you can.

Things we care about in particular:

- malformed or malicious files (images, videos, XMP, DOP) crashing or
  misleading the parsers;
- access to files outside the folders the user picked;
- loss or corruption of the catalog or of original files;
- weaknesses in the webview (CSP, exposed Tauri commands).

This is a small volunteer project: no bounty, no SLA, but every report gets
read and you'll be credited if you want.
