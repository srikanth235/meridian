// Strip non-English Chromium locale .pak files from the packaged .app.
// electron-builder's `electronLanguages` config is unreliable for macOS, so
// we delete the unwanted .lproj directories ourselves. Saves ~40 MB.
//
// Note: build/icon.png and build/icon.icns are rasterized from build/icon.svg
// using macOS's WebKit-backed `qlmanage` (ImageMagick's SVG renderer drops
// `<g>` stroke inheritance and produces an all-black image). To regenerate
// after editing icon.svg, see desktop/build/regenerate-icon.sh.

const fs = require('node:fs');
const path = require('node:path');

const KEEP = new Set(['en.lproj', 'en_GB.lproj']);

exports.default = async function afterPack(context) {
  const productName = context.packager.appInfo.productFilename;
  const localesDir = path.join(
    context.appOutDir,
    `${productName}.app`,
    'Contents/Frameworks/Electron Framework.framework/Versions/A/Resources'
  );
  if (!fs.existsSync(localesDir)) {
    console.warn('[afterPack] locales dir not found at', localesDir);
    return;
  }
  let removed = 0;
  let kept = 0;
  for (const entry of fs.readdirSync(localesDir)) {
    if (!entry.endsWith('.lproj')) continue;
    if (KEEP.has(entry)) {
      kept += 1;
      continue;
    }
    fs.rmSync(path.join(localesDir, entry), { recursive: true, force: true });
    removed += 1;
  }
  console.log(`[afterPack] locale prune: kept=${kept} removed=${removed}`);
};
