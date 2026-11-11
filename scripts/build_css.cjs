// The design uses plain CSS; keep Cargo's embedded asset synchronized without npm dependencies.
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const build = () => {
  const source = fs.readFileSync(path.join(root, 'styles/input.css'), 'utf8');
  if (process.argv.includes('--check')) {
    const built = fs.readFileSync(path.join(root, 'static/css/site.css'), 'utf8');
    if (built !== source) throw new Error('Embedded CSS is stale. Run npm run build:css.');
    console.log('PASS: embedded CSS matches styles/input.css');
    return;
  }
  fs.mkdirSync(path.join(root, 'static/css'), {recursive: true});
  fs.writeFileSync(path.join(root, 'static/css/site.css'), source);
  console.log('Built static/css/site.css');
};
build();
if (process.argv.includes('--watch')) fs.watch(path.join(root, 'styles'), (_, name) => { if (name === 'input.css') build(); });
