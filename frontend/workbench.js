(function () {
  'use strict';
  const state = { fields: {}, favorites: [], presets: [], pendingRender: null, active: null, restored: false, saveChain: Promise.resolve(), timer: null, libraryOffset: 0, libraryTotal: 0, thumbnails: new Map(), thumbnailQueue: [], thumbnailBusy: false, pickerTarget: 'tpl', restoredControls: new Set() };
  const node = id => document.getElementById(id);
  function button(label, action, className = 'sec') { const el = document.createElement('button'); el.type = 'button'; el.className = className; el.textContent = label; el.addEventListener('click', action); return el; }
  function status(message, error = false) { node('batchTaskStatus').textContent = message; node('batchTaskStatus').className = 'action-status' + (error ? ' error' : ''); }
  function batchValues() { return [...document.querySelectorAll('#bList .bi-item')].map(el => ({ title: el.querySelector('.bi-title').value, body: el.querySelector('.bi-body').value, tags: el.querySelector('.bi-tags').value })); }
  function captureFields() { return Object.fromEntries(WorkspaceFields.map(id => { const el = node(id); return [id, el?.type === 'checkbox' ? el.checked : el?.value || '']; })); }
  function capture() { return { version: 1, fields: captureFields(), palette: { ...cur }, batch: batchValues(), favorites: state.favorites, presets: state.presets, mix: { ...templateMixCounts }, pendingRender: state.pendingRender, updatedAt: Date.now() }; }
  function save(immediate = false) {
    if (!state.restored) return;
    clearTimeout(state.timer);
    node('draftStatus').textContent = '草稿保存中…';
    const commit = () => {
      const workspace = capture();
      state.saveChain = state.saveChain.catch(() => {}).then(async () => {
        try { if (nativeReady) await nativeInvoke('save_workspace', { workspace }); else localStorage.setItem('billiards-workspace-v1', JSON.stringify(workspace)); node('draftStatus').textContent = '草稿已保存 · ' + new Date().toLocaleTimeString(); }
        catch (error) { node('draftStatus').textContent = '草稿保存失败：' + String(error); }
      });
    };
    if (immediate) commit(); else state.timer = setTimeout(commit, 850);
  }
  function applyFields(fields, regenerate = false) {
    for (const [id, value] of Object.entries(fields || {})) {
      if (!WorkspaceFields.includes(id)) continue;
      const el = node(id); if (!el) continue;
      if (el.type === 'checkbox') el.checked = value === true;
      else if (el.tagName !== 'SELECT' || [...el.options].some(option => option.value === String(value))) el.value = String(value);
    }
    if (regenerate) { refreshPromptFromConfig('g', 'promptView'); refreshPromptFromConfig('b', 'bPrompt'); }
    node('bTplUniformWrap').hidden = node('bTplMode').value !== 'single'; node('bTplMixWrap').hidden = node('bTplMode').value !== 'mix';
    updateMatchFields('g'); updateMatchFields('b'); syncTextColorInputs(); syncTextColorInputs('b'); refreshThemeColors(); updateGrad(); renderTemplateMix(); updateTemplateMixSummary(); updateSingleCapacity(); updateSingleFitStates(); updateBatchCapacityStatuses(); scheduleRender();
  }
  function paintBatch(items) {
    const list = node('bList'); list.replaceChildren();
    const template = selectedBatchAiTemplate();
    items.forEach((item, index) => {
      const card = document.createElement('div'); card.className = 'bi-item lib-entry';
      const heading = document.createElement('div'); heading.textContent = `第 ${index + 1} 条`;
      const capacity = document.createElement('div'); capacity.className = 'capacity-status bi-capacity';
      const title = document.createElement('input'); title.className = 'bi-title'; title.value = item.title;
      const body = document.createElement('textarea'); body.className = 'bi-body'; body.value = item.body; body.rows = 4;
      const tags = document.createElement('input'); tags.className = 'bi-tags'; tags.value = item.tags;
      [title, body, tags].forEach(input => { input.style.width = '100%'; input.style.marginTop = '6px'; input.addEventListener('input', () => { updateBatchCapacityStatuses(); save(); }); });
      card.append(heading, capacity, title, body, tags, attachBatchFit(card, template, title, body, tags));
      const controls = document.createElement('div'); controls.className = 'lib-controls';
      controls.append(button('保存到文案库', () => addLib(title.value, body.value, tags.value)), button('移除此条', () => { card.remove(); updateBatchCapacityStatuses(); save(); refreshActions(); }));
      card.append(controls); list.append(card);
    });
    resetRandomTemplatePlan(); updateTemplateMixSummary(); updateBatchCapacityStatuses(); refreshActions();
  }
  function refreshActions() {
    const busy = Boolean(state.active), count = Math.min(100, Math.max(1, Number(node('bCount').value) || 10));
    ['batchGenerateCopyButton', 'batchRenderButton', 'batchFillButton', 'batchRetryButton'].forEach(id => node(id).disabled = busy);
    node('batchCancelButton').hidden = !busy; node('batchCancelButton').disabled = false;
    node('batchFillButton').hidden = batchValues().length >= count || !batchValues().length;
    node('batchRetryButton').hidden = !state.pendingRender || !(state.pendingRender.remaining?.length || state.pendingRender.failed?.length);
  }
  function jobId() {
    if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128;
    const hex = [...bytes].map(byte => byte.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }
  async function generate(btn, supplement = false) {
    if (state.active) { status('当前任务尚未结束'); return; }
    const prompt = node('bPrompt').value.trim(); if (!prompt) { setActionStatus('batchAiStatus', '请先生成提示词', 'error'); return; }
    if (!nativeReady) { status('请在桌面应用中生成文案', true); return; }
    const existing = supplement ? batchValues() : [];
    if (!supplement && batchValues().length && !await confirmAction('重新生成会替换当前批量文案，继续？')) return;
    const count = Math.min(100, Math.max(1, Number(node('bCount').value) || 10));
    const id = jobId(); state.active = { id, kind: 'copy' }; refreshActions(); status(`正在分组生成 ${existing.length} / ${count} 条`);
    let remove;
    try {
      if (nativeListen) remove = await nativeListen('copy-progress', event => {
        if (event.payload.jobId !== id) return;
        paintBatch(event.payload.items); status(`已生成 ${event.payload.items.length} / ${count} 条 · 自动去重与补齐`); save(true);
      });
      const result = await nativeInvoke('generate_batch_copy', { prompt, count, template: selectedBatchAiTemplate(), jobId: id, existing });
      if (result.items.length || supplement) paintBatch(result.items);
      status(`${result.cancelled ? '已取消，保留' : '完成'} ${result.items.length} / ${count} 条${result.warning ? ' · ' + result.warning : ''}`, Boolean(result.warning));
      setActionStatus('batchAiStatus', `合格且不重复：${result.items.length} / ${count} 条`, result.items.length === count ? 'success' : ''); save(true);
    } catch (error) { status('生成失败，已完成内容保留：' + String(error), true); }
    finally { if (remove) remove(); state.active = null; refreshActions(); }
  }
  function renderRequests() {
    const source = [...document.querySelectorAll('#bList .bi-item')].filter(el => el.querySelector('.bi-title').value.trim()).slice(0, 100);
    const plan = batchTemplatePlan(source.length); if (plan.error) throw new Error(plan.error);
    if (source.some(el => el.dataset.fitOver === '1')) throw new Error('有文案超出模板容量，请先缩短或换模板，避免省略关键条件');
    const ci = Number(node('bColor').value), theme = node('bTheme').value;
    return source.map((el, index) => { const color = SC[ci >= 0 ? ci : index % SC.length]; return { colorMode: ci === -2 ? 'theme' : 'custom', template: plan.templates[index], num: String(index + 1).padStart(2, '0'), tag: TAGMAP[theme] || 'BILLIARDS', title: el.querySelector('.bi-title').value, body: el.querySelector('.bi-body').value, tags: el.querySelector('.bi-tags').value, glow1: color.g1, glow2: color.g2, accent: color.a, subfolder: node('bSubfolder').value.trim(), ...typographyPayload('b') }; });
  }
  async function render(retry = false) {
    if (state.active) { status('当前任务尚未结束'); return; }
    if (!nativeReady) { status('请在桌面应用中渲染图片', true); return; }
    let requests;
    try { requests = retry ? [...(state.pendingRender?.failed || []).map(entry => entry.request || entry), ...(state.pendingRender?.remaining || [])] : renderRequests(); }
    catch (error) { status(String(error), true); return; }
    if (!requests?.length) { status('请先生成文案'); return; }
    const id = jobId(); state.active = { id, kind: 'render' }; state.pendingRender = { jobId: id, remaining: requests, failed: [] }; refreshActions(); save(true);
    const removers = []; const failures = [];
    const progress = payload => { if (payload.jobId !== id) return; state.pendingRender.remaining = requests.slice(payload.completed); state.pendingRender.failed = [...failures]; status(`已处理 ${payload.completed} / ${requests.length} 张 · 失败 ${failures.length}`); save(true); };
    try {
      if (nativeListen) {
        removers.push(await nativeListen('job-progress', event => progress(event.payload)), await nativeListen('job-failed', event => {
          if (event.payload.jobId !== id) return;
          failures.push({ request: requests[event.payload.completed - 1], error: event.payload.error }); progress(event.payload);
        }));
      }
      status(`正在渲染 0 / ${requests.length} 张`);
      const result = await nativeInvoke('render_batch', { requests, outputDir: null, jobId: id });
      state.pendingRender = { jobId: id, remaining: requests.slice(result.completed), failed: result.failed };
      const report = `${result.cancelled ? '已取消' : retry ? '重试完成' : '完成'} · 成功 ${result.outputs.length} / ${requests.length} 张 · 失败 ${result.failed.length} 张`;
      status(report, result.failed.length > 0); save(true); try { await displayOutputs(result.outputs); await loadHistory(); } catch (error) { status(report + ' · 图片已保存，预览读取失败：' + String(error)); }
    } catch (error) { status('渲染失败，未完成请求已保留：' + String(error), true); save(true); }
    finally { removers.forEach(remove => remove()); state.active = null; refreshActions(); }
  }
  async function displayOutputs(outputs) {
    const grid = node('batchGrid'); grid.replaceChildren();
    const history = await nativeInvoke('get_render_history');
    for (const output of outputs.slice(-24).reverse()) {
      const entry = history.find(entry => entry.fileName === output.fileName); if (!entry) continue;
      try { const data = await nativeInvoke('read_history_image', { historyId: entry.id }); const card = document.createElement('div'); card.className = 'batch-item'; const image = document.createElement('img'); image.src = 'data:image/png;base64,' + data; image.addEventListener('click', () => openLightbox(image.src)); card.append(image, button('打开输出目录', () => openFolder())); grid.append(card); } catch (_) {}
    }
  }
  async function cancel() {
    if (!state.active) return;
    node('batchCancelButton').disabled = true; status(state.active.kind === 'render' ? '正在取消；当前图片保存完成后停止' : '正在取消后续生成，已返回文案会保留');
    try { await nativeInvoke('cancel_job', { jobId: state.active.id }); } catch (error) { status('取消请求：' + String(error)); }
  }
  function createPicker() {
    const dialog = document.createElement('dialog'); dialog.id = 'templatePicker'; dialog.className = 'wb-dialog';
    dialog.innerHTML = '<div class="wb-toolbar"><strong>模板预览</strong><input id="templateSearch" placeholder="搜索模板名称"><select id="templateCategory"><option value="">全部分类</option></select><label><input id="templateFavoritesOnly" type="checkbox">仅收藏</label><button id="closeTemplatePicker" type="button" class="sec">关闭</button></div><div id="templateGallery" class="template-gallery"></div>';
    document.body.append(dialog);
    for (const category of new Set(TemplateCatalog.map(item => item.category))) { const option = document.createElement('option'); option.value = category; option.textContent = category; node('templateCategory').append(option); }
    ['templateSearch', 'templateCategory', 'templateFavoritesOnly'].forEach(id => node(id).addEventListener('input', paintPicker));
    node('closeTemplatePicker').addEventListener('click', () => dialog.close()); dialog.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });
  }
  function paintPicker() {
    const gallery = node('templateGallery'); observer.disconnect(); gallery.replaceChildren();
    const query = node('templateSearch').value.trim().toLowerCase(), category = node('templateCategory').value, onlyFavorites = node('templateFavoritesOnly').checked;
    for (const spec of TemplateCatalog.filter(spec => (!query || (spec.name + spec.id).toLowerCase().includes(query)) && (!category || spec.category === category) && (!onlyFavorites || state.favorites.includes(spec.id)))) {
      const card = document.createElement('div'); card.className = 'template-card' + (node(state.pickerTarget).value === spec.id ? ' selected' : '');
      const choose = button('', () => { if (state.pickerTarget === 'bTpl') node('bTplMode').value = 'single'; node(state.pickerTarget).value = spec.id; node(state.pickerTarget).dispatchEvent(new Event('change', { bubbles: true })); if (state.pickerTarget === 'bTpl') node('bTplMode').dispatchEvent(new Event('change', { bubbles: true })); node('templatePicker').close(); save(); }, 'choose');
      const img = document.createElement('img'); img.alt = spec.name + '真实模板缩略图'; img.dataset.template = spec.id;
      const name = document.createElement('span'); name.className = 'name'; name.textContent = spec.name;
      const label = document.createElement('span'); label.className = 'category'; label.textContent = spec.category;
      choose.append(img, name, label);
      const star = button(state.favorites.includes(spec.id) ? '★' : '☆', () => { state.favorites = state.favorites.includes(spec.id) ? state.favorites.filter(id => id !== spec.id) : [...state.favorites, spec.id]; paintPicker(); save(); }, 'star'); star.setAttribute('aria-label', '收藏 ' + spec.name);
      card.append(choose, star); gallery.append(card);
      if (state.thumbnails.has(spec.id)) img.src = state.thumbnails.get(spec.id); else if (nativeReady) observer.observe(img);
    }
  }
  const observer = new IntersectionObserver(entries => { entries.forEach(entry => { if (entry.isIntersecting) { observer.unobserve(entry.target); state.thumbnailQueue.push(entry.target); } }); pumpThumbnails(); }, { rootMargin: '50px' });
  async function pumpThumbnails() {
    if (state.thumbnailBusy) return; state.thumbnailBusy = true;
    while (state.thumbnailQueue.length) {
      const img = state.thumbnailQueue.shift(); if (!img.isConnected || !node('templatePicker').open) continue;
      const id = img.dataset.template;
      try { const src = state.thumbnails.get(id) || 'data:image/png;base64,' + await nativeInvoke('template_thumbnail', { template: id }); state.thumbnails.set(id, src); if (img.isConnected) img.src = src; }
      catch (_) { img.alt += '（加载失败，可关闭后重试）'; }
    }
    state.thumbnailBusy = false;
  }
  function openPicker(target) { state.pickerTarget = target; paintPicker(); node('templatePicker').showModal(); }
  function presetOptions() { const select = node('workspacePreset'); select.replaceChildren(new Option('选择常用配置', '')); state.presets.forEach(preset => select.add(new Option(preset.name, preset.id))); }
  function confirmAction(message) {
    return new Promise(resolve => {
      const dialog = document.createElement('dialog'); dialog.className = 'wb-dialog';
      const text = document.createElement('p'); text.textContent = message;
      const controls = document.createElement('div'); controls.className = 'wb-toolbar'; let accepted = false;
      controls.append(button('继续', () => { accepted = true; dialog.close(); }), button('取消', () => dialog.close()));
      dialog.append(text, controls); dialog.addEventListener('close', () => { dialog.remove(); resolve(accepted); }); document.body.append(dialog); dialog.showModal();
    });
  }
  function savePreset() {
    if (state.presets.length >= 20) { node('draftStatus').textContent = '最多保存 20 个常用配置，请先删除不常用的'; return; }
    const dialog = document.createElement('dialog'); dialog.className = 'wb-dialog';
    const label = document.createElement('label'); label.textContent = '常用配置名称'; label.htmlFor = 'newPresetName';
    const input = document.createElement('input'); input.id = 'newPresetName'; input.maxLength = 30; input.placeholder = '例如：低档比赛找搭子';
    const controls = document.createElement('div'); controls.className = 'wb-toolbar';
    controls.append(button('保存配置', () => {
      const name = input.value.trim(); if (!name) { input.focus(); return; }
      const fields = captureFields(); ['title', 'bodytxt', 'tags', 'num', 'promptView', 'bPrompt'].forEach(id => delete fields[id]);
      state.presets.push({ id: jobId(), name, fields, palette: { ...cur }, mix: { ...templateMixCounts } }); presetOptions(); save(true); dialog.close();
    }), button('取消', () => dialog.close()));
    dialog.append(label, input, controls); dialog.addEventListener('close', () => dialog.remove()); document.body.append(dialog); dialog.showModal(); input.focus();
  }
  function setupLibrary() {
    const list = node('libList'); if (!list) return;
    const controls = document.createElement('div'); controls.innerHTML = '<input id="librarySearch" class="lib-search" placeholder="搜索标题、正文或话题"><div class="lib-filter"><select id="libraryCategory"><option value="">全部分类</option><option>找搭子</option><option>赛前练球</option><option>吐槽</option><option>复盘</option><option>其他</option></select><label><input id="libraryFavoritesOnly" type="checkbox">收藏</label></div><button id="libraryExport" type="button" class="sec">导出当前筛选 CSV / JSON</button>';
    list.before(controls);
    let searchTimer; ['librarySearch', 'libraryCategory', 'libraryFavoritesOnly'].forEach(id => node(id).addEventListener('input', () => { clearTimeout(searchTimer); searchTimer = setTimeout(() => { state.libraryOffset = 0; library(); }, 250); }));
    node('libraryExport').addEventListener('click', async () => { try { const path = await nativeInvoke('export_library_entries', libraryFilter()); await nativeInvoke('open_output_folder', { path }); } catch (error) { alert('导出失败：' + String(error)); } });
  }
  function libraryFilter() { return { query: node('librarySearch')?.value.trim() || '', category: node('libraryCategory')?.value || '', favoritesOnly: node('libraryFavoritesOnly')?.checked === true }; }
  let libraryRevision = 0;
  async function library() {
    if (!nativeReady || !node('librarySearch')) return;
    const revision = ++libraryRevision, list = node('libList');
    try {
      const page = await nativeInvoke('list_library_entries', { ...libraryFilter(), offset: state.libraryOffset }); if (revision !== libraryRevision) return;
      state.libraryTotal = page.total; list.replaceChildren();
      if (!page.entries.length && state.libraryOffset > 0) { state.libraryOffset = Math.max(0, state.libraryOffset - 50); return library(); }
      if (!page.entries.length) { list.textContent = '暂无匹配文案'; return; }
      for (const entry of page.entries) {
        const card = document.createElement('div'); card.className = 'lib-entry'; const title = document.createElement('strong'); title.textContent = entry.title;
        const body = document.createElement('p'); body.textContent = entry.body;
        const label = document.createElement('div'); label.textContent = (entry.category || '未分类') + ' · ' + entry.tags; label.style.color = 'var(--t3)';
        const controls = document.createElement('div'); controls.className = 'lib-controls'; controls.append(button('使用', () => { node('title').value = entry.title; node('bodytxt').value = entry.body; node('tags').value = entry.tags; showTab(0); updateSingleCapacity(); updateSingleFitStates(); scheduleRender(); save(); }), button(entry.favorite ? '★ 已收藏' : '☆ 收藏', async () => { try { await nativeInvoke('update_library_entry', { entry: { ...entry, favorite: !entry.favorite } }); await library(); } catch (error) { alert(String(error)); } }), button('编辑 / 分类', () => editLibrary(entry)), button('删除', async () => { if (!await confirmAction('删除这条本地文案？')) return; try { await nativeInvoke('delete_library_entry', { id: entry.id }); await library(); } catch (error) { alert(String(error)); } }));
        card.append(title, body, label, controls); list.append(card);
      }
      const pagination = document.createElement('div'); pagination.className = 'lib-pagination'; const previous = button('上一页', () => { state.libraryOffset = Math.max(0, state.libraryOffset - 50); library(); }); previous.disabled = state.libraryOffset === 0;
      const label = document.createElement('span'); label.textContent = `${state.libraryOffset + 1}–${state.libraryOffset + page.entries.length} / ${page.total}`;
      const next = button('下一页', () => { state.libraryOffset += 50; library(); }); next.disabled = state.libraryOffset + 50 >= page.total; pagination.append(previous, label, next); list.append(pagination);
    } catch (error) { if (revision === libraryRevision) list.textContent = '文案库读取失败：' + String(error); }
  }
  function editLibrary(entry) {
    const dialog = document.createElement('dialog'); dialog.className = 'wb-dialog'; dialog.innerHTML = '<h3>编辑文案</h3><div class="field"><label>标题</label><input data-field="title"></div><div class="field"><label>正文</label><textarea data-field="body"></textarea></div><div class="field"><label>话题</label><input data-field="tags"></div><div class="field"><label>分类</label><select data-field="category"><option value="">未分类</option><option>找搭子</option><option>赛前练球</option><option>吐槽</option><option>复盘</option><option>其他</option></select></div>';
    ['title', 'body', 'tags', 'category'].forEach(key => dialog.querySelector(`[data-field="${key}"]`).value = entry[key]);
    const controls = document.createElement('div'); controls.className = 'wb-toolbar'; controls.append(button('保存修改', async () => { const updated = { ...entry }; ['title', 'body', 'tags', 'category'].forEach(key => updated[key] = dialog.querySelector(`[data-field="${key}"]`).value); try { await nativeInvoke('update_library_entry', { entry: updated }); dialog.close(); await library(); } catch (error) { alert(String(error)); } }), button('取消', () => dialog.close())); dialog.append(controls); dialog.addEventListener('close', () => dialog.remove()); document.body.append(dialog); dialog.showModal();
  }
  function onTemplates() { if (state.restored) { ['tpl', 'bTpl', 'preset'].forEach(id => { if (!state.restoredControls.has(id) && state.fields[id] && [...node(id).options].some(option => option.value === state.fields[id])) { node(id).value = state.fields[id]; state.restoredControls.add(id); } }); refreshThemeColors(); updateSingleCapacity(); updateSingleFitStates(); } }
  async function init() {
    createPicker(); setupLibrary();
    node('openTemplatePicker').addEventListener('click', () => openPicker('tpl')); node('openBatchTemplatePicker').addEventListener('click', () => openPicker('bTpl'));
    node('batchCancelButton').addEventListener('click', cancel); node('batchFillButton').addEventListener('click', () => generate(node('batchFillButton'), true)); node('batchRetryButton').addEventListener('click', () => render(true));
    node('saveWorkspacePreset').addEventListener('click', savePreset); node('workspacePreset').addEventListener('change', () => { const preset = state.presets.find(preset => preset.id === node('workspacePreset').value); if (!preset) return; templateMixCounts = { ...preset.mix }; if (preset.palette) cur = { ...preset.palette }; applyFields(preset.fields, true); save(); });
    node('deleteWorkspacePreset').addEventListener('click', () => { const id = node('workspacePreset').value; if (!id) return; state.presets = state.presets.filter(preset => preset.id !== id); presetOptions(); save(true); });
    try {
      const draft = nativeReady ? await nativeInvoke('load_workspace') : JSON.parse(localStorage.getItem('billiards-workspace-v1') || 'null');
      if (draft?.version === 1) { state.fields = draft.fields || {}; state.favorites = (draft.favorites || []).filter(id => TemplateCatalog.some(spec => spec.id === id)); state.presets = (draft.presets || []).slice(0, 20); state.pendingRender = draft.pendingRender || null;
        if (nativeReady && state.pendingRender?.jobId) {
          const checkpoint = await nativeInvoke('load_render_checkpoint', { jobId: state.pendingRender.jobId });
          if (checkpoint) state.pendingRender = { jobId: state.pendingRender.jobId, remaining: checkpoint.requests.slice(checkpoint.completed), failed: checkpoint.failed };
        }
        templateMixCounts = { ...draft.mix }; if (draft.palette) cur = { ...draft.palette }; applyFields(state.fields); paintBatch(draft.batch || []); node('draftStatus').textContent = '已恢复上次草稿'; }
      state.restored = true; presetOptions(); refreshActions(); onTemplates(); await library();
    } catch (error) { node('draftStatus').textContent = '草稿恢复失败：' + String(error); state.restored = true; }
    document.addEventListener('input', event => { if (WorkspaceFields.includes(event.target.id)) save(); }); document.addEventListener('change', event => { if (WorkspaceFields.includes(event.target.id)) { save(); refreshActions(); } });
    window.addEventListener('beforeunload', () => save(true));
  }
  window.Workbench = { generate, render, library, onTemplates, get restored() { return state.restored; } };
  init();
})();
