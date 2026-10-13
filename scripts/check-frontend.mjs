import { webcrypto } from 'node:crypto';
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const catalog = JSON.parse(fs.readFileSync('shared/template-catalog.json','utf8'));
const fields = JSON.parse(fs.readFileSync('frontend/workspace-fields.json','utf8'));
const sandbox=vm.createContext({});
for(const file of ['template-catalog','workspace-fields','match-prompts','workbench']) {
  const source=fs.readFileSync('frontend/'+file+'.js','utf8');
  new vm.Script(source,{filename:file+'.js'});
  if(['template-catalog','workspace-fields'].includes(file))vm.runInContext(source,sandbox);
}
assert.deepEqual(JSON.parse(JSON.stringify(sandbox.TemplateCatalog)),catalog);
assert.deepEqual(JSON.parse(JSON.stringify(sandbox.WorkspaceFields)),fields);
assert.equal(new Set(catalog.map(item=>item.id)).size,50);
assert(!fields.some(id=>/key|password|token/i.test(id)));
const html=fs.readFileSync('frontend/index.html','utf8');
for(const m of html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g))new vm.Script(m[1]);
for(const id of fields)assert(html.includes('id="'+id+'"'),'Missing draft field: '+id);
console.log('Frontend scripts, 50-template shared catalog and non-secret draft fields passed');

const inputs={customTextColors:{checked:true},titleColor:{disabled:true},bodyColor:{disabled:true},tagColor:{disabled:true},bCustomTextColors:{checked:false},bTitleColor:{disabled:false},bBodyColor:{disabled:false},bTagColor:{disabled:false}};
sandbox.document={getElementById:id=>inputs[id]};
vm.runInContext(html.match(/function syncTextColorInputs[^\n]+/)[0],sandbox);
vm.runInContext('syncTextColorInputs();syncTextColorInputs("b")',sandbox);
assert.equal(inputs.titleColor.disabled,false);assert.equal(inputs.bodyColor.disabled,false);assert.equal(inputs.tagColor.disabled,false);
assert.equal(inputs.bTitleColor.disabled,true);assert.equal(inputs.bBodyColor.disabled,true);assert.equal(inputs.bTagColor.disabled,true);
const workbench=fs.readFileSync('frontend/workbench.js','utf8');
const uuidSource=workbench.slice(workbench.indexOf('  function jobId()'),workbench.indexOf('  async function generate'));
sandbox.crypto={getRandomValues:webcrypto.getRandomValues.bind(webcrypto)};
vm.runInContext(uuidSource,sandbox);
assert.match(vm.runInContext('jobId()',sandbox),/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
console.log('Single/batch text color toggles and legacy WebView task IDs passed');
