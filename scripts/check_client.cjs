// State-oriented checks for production assets using a small DOM stub.
const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const root = require('node:path').resolve(__dirname, '..');
class Element {
  constructor(value = '') { this.defaultValue = value; this.value = value; this.textContent = ''; this.dataset = {}; this.listeners = {}; this.attrs = {}; }
  addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
  emit(type, event = {}) { for (const fn of this.listeners[type] || []) fn({target: this, ...event}); }
  setAttribute(k,v) { this.attrs[k]=v; }
  getAttribute(k) { return this.attrs[k]; }
  hasAttribute(k) { return k in this.attrs; }
  removeAttribute(k) { delete this.attrs[k]; }
  setCustomValidity(message) { this.validationMessage = message; }
  remove() { this.removed = true; }
  focus() { this.focused = true; }
}
function storage(seed = {}) {
  const store = {...seed};
  Object.defineProperties(store, {
    getItem: {value(k){return this[k] ?? null;}}, setItem: {value(k,v){this[k]=v;}}, removeItem: {value(k){delete this[k];}}
  });
  return store;
}
function fixture({drafts, template = false, choice = 'system', storageBlocked = false, recipe = null, collection = false, snackbar = false, modal = false, customTheme, appearance = true} = {}) {
  const doc = new Element(), win = new Element(), nodes = {}, timers = new Map(), buttons = [];
  const editor = new Element('Saved {{title}}');
  editor.dataset.defaultSource = 'Default {{title}}';
  const status = new Element(), preview = new Element(), message = new Element();
  const form = new Element(); form.dataset.owner = '1';
  form.elements = {source:editor,format:{value:'text'},csrf:{value:'a'.repeat(64)}};
  const reset = new Element(), discard = new Element();
  form.querySelector = selector => selector === '[data-template-reset]' ? reset : discard;
  form.fields = () => Object.entries(form.elements).map(([key,node]) => [key,node.value]);
  const nav = new Element(), templateLink = new Element(); nav.dataset.owner = '1';nav.querySelector=()=>templateLink;
  for (const name of ['light','dark','system']) { const button=new Element();button.dataset.themeChoice=name;buttons.push(button); }
  const properties = new Map();
  const customToggle = new Element(), customEditor = new Element(), customReset = new Element();
  customToggle.dataset.customThemeToggle = 'true';
  const customFields = [];
  for (const mode of ['light', 'dark']) for (const color of ['foreground', 'background', 'accent', 'danger']) {
    for (const type of ['color', 'text']) {
      const input = new Element('#123456');
      input.type = type; input.dataset.customThemeMode = mode; input.dataset.customThemeColor = color;
      customFields.push(input);
    }
  }
  doc.documentElement={dataset:{},classList:{add(){}},style:{setProperty(key,value){properties.set(key,value);},removeProperty(key){properties.delete(key);}}};
  doc.querySelector = selector => ({'[data-template-form]':template?form:null,'.settings-tabs':nav,'[data-custom-theme-toggle]':appearance?customToggle:null,'[data-custom-theme-editor]':appearance?customEditor:null})[selector] || null;
  doc.querySelectorAll = selector => selector === 'button[data-theme-choice]' ? buttons : selector === '[data-custom-theme-color]' && appearance ? customFields : [];
  doc.getElementById = id => ({'template-status':status,'template-preview':preview,'preview-message':message})[id] || null;
  const session = storage(drafts ? {'recipe-shelf:templates:1':JSON.stringify(drafts)} : {});
  const local = storage({'recipe-shelf-theme':choice});
  if(customTheme !== undefined) local['recipe-shelf-custom-theme'] = customTheme;
  if(storageBlocked) local.getItem = undefined;
  const media = new Element();media.matches=true;
  const requests=[], prompts=[], transitions=[];
  let confirmResult=true, nextTimer=1;
  const recipeChoice=recipe === null ? null : new Element(recipe);
  if (recipeChoice) { recipeChoice.dataset.defaultRecipe='1'; form.elements.recipe=new Element(recipe); }
  const formatTabs=['text','markdown','pdf'].map(kind=>{const link=new Element();link.href=`http://localhost/settings/templates?format=${kind}`;link.textContent=kind;return link;});
  const exportForm=new Element(); exportForm.elements={format:new Element('text'),recipe:new Element('2')};
  exportForm.fields=()=>Object.entries(exportForm.elements).map(([key,node])=>[key,node.value]);
  exportForm.querySelector=()=>null;
  const exportOutput=new Element(), exportMessage=new Element(), customize=new Element();
  const snack=new Element(), dismiss=new Element();snack.querySelector=()=>dismiss;
  const dialog=new Element();dialog.id='user-dialog';dialog.open=true;
  dialog.showModal=()=>{dialog.open=true;dialog.modalActive=true;};
  dialog.close=()=>{dialog.open=false;dialog.emit('close');};
  const opener=new Element();
  const originalQuery=doc.querySelector;
  doc.querySelector=selector=>selector==='[data-modal-link="user-dialog"]'?opener:originalQuery(selector);
  const originalAll=doc.querySelectorAll;
  doc.querySelectorAll=selector=>selector==='.format-tabs a'?formatTabs:selector==='[data-snackbar]'?(snackbar?[snack]:[]):selector==='dialog[data-modal][open]'?(modal?[dialog]:[]):originalAll(selector);
  const originalID=doc.getElementById;
  doc.getElementById=id=>({'template-preview-recipe':recipeChoice,'collection-export':collection?exportForm:null,'export-preview-recipe':collection?exportForm.elements.recipe:null,'export-preview':exportOutput,'export-preview-message':exportMessage,'customize-export-template':customize})[id]||originalID(id);
  const context=vm.createContext({document:doc,window:win,sessionStorage:session,
    localStorage:storageBlocked?{getItem(){throw Error('blocked');},setItem(){throw Error('blocked');}}:local,
    location:new URL('http://localhost/settings/templates?format=text'), URL, URLSearchParams, AbortController,
    matchMedia(){return media;},confirm(message){prompts.push(message);return confirmResult;},history:{replaceState(...args){transitions.push(args[2]);}},
    setTimeout(fn){const id=nextTimer++;timers.set(id,fn);return id;},clearTimeout(id){timers.delete(id);},requestAnimationFrame(fn){fn();},
    FormData: class {constructor(form){this.form=form;}[Symbol.iterator](){return this.form.fields()[Symbol.iterator]();}},
    async fetch(url,options){requests.push({url,options});return{ok:true,async json(){return{preview:'<script>shown as text</script>'};}};}
  });
  const run=file=>vm.runInContext(fs.readFileSync(`${root}/static/js/${file}`,'utf8'),context);
  return {doc,win,session,local,context,properties,customToggle,customEditor,customFields,customReset,media,buttons,editor,status,preview,message,form,reset,discard,requests,templateLink,run,prompts,transitions,recipeChoice,formatTabs,exportForm,exportOutput,customize,snack,dismiss,dialog,opener,
    setConfirm(value){confirmResult=value;},
    clickLink(path){const link=new Element();link.href=new URL(path,'http://localhost').href;link.target='';link.closest=selector=>selector==='a[href]'?link:null;let prevented=false;doc.emit('click',{target:link,button:0,detail:1,preventDefault(){prevented=true;}});return prevented;},
    flushTimers(){const pending=[...timers.values()];timers.clear();pending.forEach(fn=>fn());}};
}
(async () => {
  const theme=fixture({choice:'light'});theme.run('theme.js');
  assert.equal(theme.doc.documentElement.dataset.theme,'light');
  theme.doc.emit('DOMContentLoaded');assert.equal(theme.buttons[0].attrs['aria-pressed'],'true');
  theme.doc.emit('click',{target:{closest(){return theme.buttons[1];}}});
  assert.equal(theme.doc.documentElement.dataset.theme,'dark');
  assert.equal(theme.local['recipe-shelf-theme'],'dark');
  const blocked=fixture({storageBlocked:true});blocked.run('theme.js');assert.equal(blocked.doc.documentElement.dataset.theme,'dark');

  const custom = fixture({choice:'light'});custom.run('theme.js');custom.doc.emit('DOMContentLoaded');
  assert(custom.customEditor.hidden);
  custom.customToggle.checked = true;
  custom.doc.emit('click',{target:{closest(selector){return selector === '[data-theme-choice]' ? custom.doc.documentElement : null;}}});
  assert(custom.customToggle.checked,'the root theme attribute must not act like a theme button during checkbox clicks');
  custom.doc.emit('change',{target:custom.customToggle});
  assert(!custom.customEditor.hidden);
  const editColor = (mode,color,value,type='text') => {
    const input=custom.customFields.find(input=>input.dataset.customThemeMode===mode && input.dataset.customThemeColor===color && input.type===type);
    input.value=value;custom.doc.emit('input',{target:input});return input;
  };
  editColor('light','foreground','#334455');
  assert.equal(custom.properties.get('--foreground'),'#334455');
  assert.equal(custom.customFields.find(input=>input.type==='color' && input.dataset.customThemeMode==='light' && input.dataset.customThemeColor==='foreground').value,'#334455');
  editColor('dark','accent','#AABBCC','color');
  assert(!custom.properties.has('--accent'),'editing dark colors does not alter the active light palette');
  const invalid=editColor('light','foreground','invalid');
  assert.equal(invalid.attrs['aria-invalid'],'true');
  assert.equal(custom.properties.get('--foreground'),'#334455','invalid input leaves the last valid color in place');
  assert(!custom.local['recipe-shelf-custom-theme'].includes('invalid'));
  custom.doc.emit('click',{target:{closest(){return custom.buttons[1];}}});
  assert.equal(custom.properties.get('--accent'),'#aabbcc');
  assert(!custom.properties.has('--foreground'));
  const savedColors=custom.local['recipe-shelf-custom-theme'];
  const reload=fixture({choice:'dark',customTheme:savedColors,appearance:false});reload.run('theme.js');
  assert.equal(reload.properties.get('--accent'),'#aabbcc','custom colors apply on other pages before DOMContentLoaded');
  custom.customToggle.checked=false;custom.doc.emit('change',{target:custom.customToggle});
  assert(custom.customEditor.hidden);assert.equal(custom.properties.size,0);
  assert.equal(JSON.parse(custom.local['recipe-shelf-custom-theme']).dark.accent,'#aabbcc','disabling preserves colors');
  custom.customToggle.checked=true;custom.doc.emit('change',{target:custom.customToggle});
  assert.equal(custom.properties.get('--accent'),'#aabbcc');
  custom.doc.emit('click',{target:{closest(selector){return selector==='[data-custom-theme-reset]'?custom.customReset:null;}}});
  assert.equal(custom.properties.size,0);assert(!custom.customEditor.hidden);
  assert(custom.customFields.every(input=>input.value===input.defaultValue));
  reload.win.emit('storage',{key:'recipe-shelf-custom-theme',newValue:JSON.stringify({enabled:true,light:{foreground:'#102030'},dark:{accent:'#fedcba',background:'url(evil)'}})});
  assert.equal(reload.properties.get('--accent'),'#fedcba');assert(!reload.properties.has('--background'));
  reload.win.emit('storage',{key:'recipe-shelf-theme',newValue:'system'});
  reload.media.matches=false;reload.media.emit('change');
  assert.equal(reload.properties.get('--foreground'),'#102030');
  reload.win.emit('storage',{key:null,newValue:null});assert.equal(reload.properties.size,0);
  const corrupt=fixture({customTheme:'broken json'});corrupt.run('theme.js');assert.equal(corrupt.properties.size,0);
  blocked.customToggle.checked=true;blocked.doc.emit('change',{target:blocked.customToggle});assert(!blocked.customEditor.hidden);

  const first=fixture({template:true});first.run('app.js');
  first.editor.value='Draft {{title}}';first.editor.emit('input');
  assert.match(first.status.textContent,/Unsaved changes \* \(text\)/);
  assert.match(first.templateLink.textContent,/Templates \*/);
  assert.equal(JSON.parse(first.session['recipe-shelf:templates:1']).text.source,'Draft {{title}}');
  let warned=false;first.win.emit('beforeunload',{preventDefault(){warned=true;}});assert(warned);
  first.flushTimers();await new Promise(setImmediate);
  assert.equal(first.preview.textContent,'<script>shown as text</script>');
  assert.equal(first.preview.innerHTML,undefined,'preview must use text, never executable HTML');
  assert.equal(first.requests.at(-1).options.body.get('source'),'Draft {{title}}');
  const otherDraft={source:'Markdown draft',base:'Markdown saved'};
  const second=fixture({template:true,drafts:{text:{source:'Draft {{title}}',base:'Saved {{title}}'},markdown:otherDraft}});second.run('app.js');
  assert.equal(second.editor.value,'Draft {{title}}','navigation must restore the draft');
  second.discard.emit('click');
  assert.equal(second.editor.value,'Saved {{title}}');
  assert.deepEqual(JSON.parse(second.session['recipe-shelf:templates:1']),{markdown:otherDraft},'discard affects only this format');
  second.reset.emit('click');assert.equal(second.editor.value,'Default {{title}}');
  assert.match(second.status.textContent,/Unsaved/);
  const conflict=fixture({template:true,drafts:{text:{source:'Old draft',base:'Older saved'}}});conflict.run('app.js');
  assert.match(conflict.status.textContent,/saved version has changed/);
  const saved=fixture({template:true,drafts:{text:{source:'Saved {{title}}',base:'Older saved'}}});saved.run('app.js');
  assert.deepEqual(JSON.parse(saved.session['recipe-shelf:templates:1']),{},'a server-confirmed save clears its matching draft');
  const otherPage=fixture({drafts:{text:{source:'Draft',base:'Saved'}}});otherPage.run('app.js');
  warned=false;otherPage.win.emit('beforeunload',{preventDefault(){warned=true;}});assert(!warned,'draft prompts apply only while editing the Templates route');
  const guarded=fixture({template:true});guarded.run('app.js');
  guarded.editor.value='Unsaved new version';guarded.editor.emit('input');
  assert.match(guarded.templateLink.textContent,/\*$/);
  guarded.setConfirm(false);
  assert(guarded.clickLink('/settings'),'canceling route departure prevents navigation');
  assert(guarded.session['recipe-shelf:templates:1']);
  assert.match(guarded.prompts.at(-1),/lose these changes/);
  const prompts=guarded.prompts.length;
  assert(!guarded.clickLink('/settings/templates?format=markdown'));
  assert.equal(guarded.prompts.length,prompts,'format tabs do not leave the Templates route');
  guarded.setConfirm(true);
  assert(!guarded.clickLink('/exports'));
  assert.equal(guarded.session['recipe-shelf:templates:1'],undefined,'confirmed route departure discards drafts');

  const reloaded=fixture({template:true});reloaded.run('app.js');
  reloaded.editor.value='Reload draft';reloaded.editor.emit('input');
  reloaded.win.emit('pagehide');
  assert.equal(reloaded.session['recipe-shelf:templates:1'],undefined);
  reloaded.win.emit('pageshow');
  assert.equal(reloaded.editor.value,'Saved {{title}}','back-forward cache does not resurrect discarded edits');

  const selected=fixture({template:true,recipe:'2'});selected.run('app.js');
  await new Promise(setImmediate);
  assert.equal(selected.requests.at(-1).options.body.get('recipe'),'2');
  selected.recipeChoice.value='3';selected.recipeChoice.emit('change');
  await new Promise(setImmediate);
  assert.equal(selected.requests.at(-1).options.body.get('recipe'),'3');
  assert.equal(selected.form.elements.recipe.value,'3','saving retains the selected recipe');
  for (const tab of selected.formatTabs) assert.equal(new URL(tab.href).searchParams.get('recipe'),'3');
  selected.clickLink('/settings/templates?format=pdf');
  assert.equal(selected.recipeChoice.value,'3','switching template types keeps the recipe');
  selected.clickLink('/settings');
  assert.equal(selected.recipeChoice.value,'1','leaving the route resets the recipe');
  for (const tab of selected.formatTabs) assert(!new URL(tab.href).searchParams.has('recipe'));


  const exports=fixture({collection:true});exports.run('app.js');
  exports.exportForm.elements.format.value='pdf';exports.exportForm.emit('change');
  await new Promise(setImmediate);
  assert.match(exports.requests.at(-1).url,/format=pdf/);
  assert.match(exports.requests.at(-1).url,/recipe=2/);
  assert.equal(exports.customize.href,'/settings/templates?format=pdf');
  assert.equal(exports.exportOutput.textContent,'<script>shown as text</script>');
  assert.equal(exports.exportOutput.innerHTML,undefined,'saved previews also use text');
  exports.exportForm.elements.recipe.value='4';exports.exportForm.elements.recipe.emit('change');
  await new Promise(setImmediate);
  assert.match(exports.requests.at(-1).url,/recipe=4/,'the recipe control outside the format card refreshes its preview');


  const notification=fixture({snackbar:true});notification.run('app.js');
  notification.snack.emit('focusin');notification.flushTimers();
  assert(!notification.snack.removed,'notifications remain while focused');
  notification.snack.emit('focusout');notification.flushTimers();
  assert(notification.snack.removed,'notifications expire after focus leaves');
  const dismissible=fixture({snackbar:true});dismissible.run('app.js');dismissible.dismiss.emit('click');
  assert(dismissible.snack.removed);

  const modal=fixture({modal:true});modal.run('app.js');
  assert(modal.dialog.modalActive,'server-open forms become native modal dialogs');
  modal.dialog.close();assert(modal.opener.focused,'closing restores focus to the opener');

  console.log('PASS: themes and custom palettes (editing, validation, persistence, reset, system mode and cross-tab sync); template drafts, asterisks, discard prompts and recipe selection; saved export previews and format links; snackbar expiry/dismissal; modal activation and focus restoration.');
})().catch(error=>{console.error(error);process.exitCode=1;});
