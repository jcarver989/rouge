import html
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path


def metadata(target):
    config = json.loads(Path('.rouge/app.json').read_text())
    cargo = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--no-deps', '--format-version', '1']))
    if config['package']:
        candidates = [p for p in cargo['packages'] if p['name'] == config['package']]
    else:
        candidates = [p for p in cargo['packages'] if p['id'] in cargo['workspace_default_members']]
    if len(candidates) != 1:
        sys.exit('Select one workspace package with RougePublish::package()')
    package = candidates[0]
    if target == 'android':
        candidates = [t['name'] for t in package['targets'] if 'cdylib' in t['crate_types']]
        selected = config['android_library']
    else:
        candidates = [t['name'] for t in package['targets'] if 'bin' in t['kind']]
        selected = config['binary']
    if selected is not None:
        candidates = [name for name in candidates if name == selected]
    if len(candidates) != 1 or not re.fullmatch(r'[a-zA-Z0-9_][a-zA-Z0-9_-]*', candidates[0]):
        sys.exit('Package needs one Cargo binary (web/desktop/iOS) or cdylib (Android); use binary() or android_library() to select it')
    version = package['version']
    if target == 'ios' and not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version):
        sys.exit('iOS requires a Cargo package version in MAJOR.MINOR.PATCH format, without a suffix')
    values = {
        'APP_ID': config['id'], 'APP_NAME': config['name'] or package['name'],
        'APP_ICON': config['icon'] or '', 'APP_PACKAGE': package['name'],
        'APP_TARGET': candidates[0], 'APP_VERSION': version,
        'CARGO_LOCK': str(Path(cargo['workspace_root']) / 'Cargo.lock'),
    }
    for key, val in values.items():
        print(f'{key}={shlex.quote(val)}')


def bindgen_version(lock):
    versions = re.findall(r'^name = "wasm-bindgen"\nversion = "([0-9.]+)"$', Path(lock).read_text(), re.M)
    if len(versions) != 1:
        sys.exit('Expected exactly one wasm-bindgen version in Cargo.lock')
    print(versions[0])


def web_page(path):
    name = html.escape(os.environ['APP_NAME'])
    icon = '\n  <link rel="icon" href="./icon.png">' if os.environ['APP_ICON'] else ''
    Path(path).write_text(f'''<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>{name}</title>{icon}
  <style>html, body {{ margin: 0; height: 100%; overflow: hidden; overscroll-behavior: none; }}</style>
</head>
<body>
  <script type="module">
    import init from './app.js';
    init().catch(error => {{
      console.error(error);
      document.body.textContent = 'Could not start the app. Try a browser with WebGPU or WebGL2.';
    }});
  </script>
</body>
</html>
''')


if sys.argv[1] == 'metadata':
    metadata(sys.argv[2])
elif sys.argv[1] == 'bindgen-version':
    bindgen_version(sys.argv[2])
elif sys.argv[1] == 'web-page':
    web_page(sys.argv[2])
else:
    sys.exit('Unknown metadata command')
