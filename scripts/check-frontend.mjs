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
