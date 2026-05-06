// Tauri injects these globals because we set "withGlobalTauri": true in tauri.conf.json
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── Window controls (custom title bar) ───────────────────────────────────────

const tauriWindow = window.__TAURI__.window.getCurrentWindow();
const winMaxBtn   = document.querySelector('.win-maximize');

document.querySelector('.win-minimize').addEventListener('click', () => tauriWindow.minimize());
document.querySelector('.win-maximize').addEventListener('click', () => tauriWindow.toggleMaximize());
document.querySelector('.win-close')   .addEventListener('click', () => tauriWindow.close());

// Swap the maximize icon when the window is already maximized
tauriWindow.onResized(async () => {
  const maximized = await tauriWindow.isMaximized();
  winMaxBtn.innerHTML = maximized ? '&#x2750;' : '&#x25A1;';  // ❐ vs □
  winMaxBtn.title     = maximized ? 'Restore' : 'Maximize';
});

// ── Tab switching ────────────────────────────────────────────────────────────

document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(s => s.classList.remove('active'));
    tab.classList.add('active');
    document.getElementById(`tab-${tab.dataset.tab}`).classList.add('active');

    if (tab.dataset.tab === 'installed') loadInstalled();
  });
});

// ── Silent-install preference ────────────────────────────────────────────────
// Persisted in localStorage so it survives between sessions.
// Default is true (silent on); the user must explicitly opt out.

function getSilentDefault() {
  return localStorage.getItem('silent_default') !== 'false';
}
function setSilentDefault(val) {
  localStorage.setItem('silent_default', String(val));
}

const silentDefaultToggle = document.getElementById('silent-default-toggle');
silentDefaultToggle.checked = getSilentDefault();

silentDefaultToggle.addEventListener('change', () => {
  setSilentDefault(silentDefaultToggle.checked);
  // Sync all per-app checkboxes currently visible in the DOM
  document.querySelectorAll('.silent-check').forEach(cb => {
    cb.checked = silentDefaultToggle.checked;
  });
});

// ── Search ───────────────────────────────────────────────────────────────────

const tabSearch          = document.getElementById('tab-search');
const searchWelcome      = document.getElementById('search-welcome');
const searchInputWelcome = document.getElementById('search-input-welcome');
const searchBtnWelcome   = document.getElementById('search-btn-welcome');
const searchInput        = document.getElementById('search-input');
const searchBtn          = document.getElementById('search-btn');
const searchStatus       = document.getElementById('search-status');
const searchToolbar      = document.getElementById('search-toolbar');
const resultsGrid        = document.getElementById('results-grid');
const showMsStoreToggle  = document.getElementById('show-msstore-toggle');

let allSearchResults = [];
let lastQuery = '';

// Compact bar (post-search)
searchBtn.addEventListener('click', doSearch);
searchInput.addEventListener('keydown', e => { if (e.key === 'Enter') doSearch(); });
showMsStoreToggle.addEventListener('change', renderSearch);

// Welcome bar + quick-picks
searchBtnWelcome.addEventListener('click', doSearchFromWelcome);
searchInputWelcome.addEventListener('keydown', e => { if (e.key === 'Enter') doSearchFromWelcome(); });
document.querySelectorAll('.quick-pick').forEach(btn => {
  btn.addEventListener('click', () => {
    searchInputWelcome.value = btn.dataset.query;
    doSearchFromWelcome();
  });
});

function doSearchFromWelcome() {
  searchInput.value = searchInputWelcome.value;
  doSearch();
}

function transitionToSearched() {
  if (tabSearch.classList.contains('searched')) return;
  // Fade the welcome section out with an inline transition, then switch layouts.
  searchWelcome.style.transition = 'opacity 0.22s ease, transform 0.22s ease';
  searchWelcome.style.opacity    = '0';
  searchWelcome.style.transform  = 'translateY(-10px)';
  setTimeout(() => {
    tabSearch.classList.add('searched');
    searchWelcome.style.cssText = ''; // CSS now controls display: none
    searchInput.focus();
  }, 230);
}

function isMsStore(pkg) {
  return pkg.source === 'msstore' || /^[0-9A-Z]{9,13}$/.test(pkg.id);
}

async function doSearch() {
  const query = searchInput.value.trim();
  if (!query) return;

  transitionToSearched(); // no-op after first search
  lastQuery = query;
  searchBtn.disabled        = true;
  searchBtnWelcome.disabled = true;
  searchToolbar.classList.remove('hidden');
  searchStatus.textContent = 'Searching…';
  showSkeletons(8);

  try {
    const [results, installed] = await Promise.all([
      invoke('search_packages', { query }),
      invoke('list_installed').catch(() => []),
    ]);
    const installedIds = new Set(installed.map(p => p.id));
    allSearchResults = results.map(p => ({ ...p, installed: installedIds.has(p.id) }));
    renderSearch();
  } catch (err) {
    resultsGrid.innerHTML = '';
    searchStatus.textContent = `Error: ${err}`;
  } finally {
    searchBtn.disabled        = false;
    searchBtnWelcome.disabled = false;
  }
}

function renderSearch() {
  const showStore = showMsStoreToggle.checked;
  const packages  = showStore ? allSearchResults : allSearchResults.filter(p => !isMsStore(p));
  const hidden    = allSearchResults.length - packages.length;

  resultsGrid.innerHTML = '';

  if (packages.length === 0) {
    searchStatus.textContent = hidden > 0
      ? `No results shown · ${hidden} Microsoft Store result${hidden === 1 ? '' : 's'} hidden`
      : `No packages found for "${lastQuery}"`;
    return;
  }

  const hiddenNote = hidden > 0 ? ` · ${hidden} Store result${hidden === 1 ? '' : 's'} hidden` : '';
  searchStatus.textContent = `${packages.length} result${packages.length === 1 ? '' : 's'} for "${lastQuery}"${hiddenNote}`;
  packages.forEach(pkg => resultsGrid.appendChild(makeCard(pkg)));
}

function showSkeletons(n) {
  resultsGrid.innerHTML = '';
  for (let i = 0; i < n; i++) {
    const card = document.createElement('div');
    card.className = 'pkg-card skeleton';
    card.innerHTML = `
      <div class="skel skel-name"></div>
      <div class="skel skel-id"></div>
      <div class="skel skel-meta"></div>
      <div class="skel skel-btn"></div>
    `;
    resultsGrid.appendChild(card);
  }
}

const SOURCE_LABEL = { winget: 'winget', msstore: 'Store', brew: 'brew' };

function makeCard(pkg) {
  const card        = document.createElement('div');
  card.className    = 'pkg-card';
  const label       = SOURCE_LABEL[pkg.source] ?? pkg.source;
  const badgeHtml   = pkg.source
    ? `<span class="source-badge source-${escHtml(pkg.source)}">${escHtml(label)}</span>`
    : '';
  const versionHtml = pkg.version ? `<span class="pkg-meta">${escHtml(pkg.version)}</span>` : '';

  const btnClass = pkg.installed ? 'install-btn installed' : 'install-btn';
  const btnLabel = pkg.installed ? 'Already Installed' : 'Install';
  const silentHtml = !pkg.installed
    ? `<label class="silent-label">
         <input type="checkbox" class="silent-check"${getSilentDefault() ? ' checked' : ''}>
         Silent
       </label>`
    : '';

  card.innerHTML = `
    <div class="pkg-name" title="${escHtml(pkg.name)}">${escHtml(pkg.name)}</div>
    <div class="pkg-id"  title="${escHtml(pkg.id)}">${escHtml(pkg.id)}</div>
    <div class="pkg-footer">${versionHtml}${badgeHtml}</div>
    <div class="card-actions">
      <button class="${btnClass}">${btnLabel}</button>
      ${silentHtml}
    </div>
  `;
  if (!pkg.installed) {
    card.querySelector('.install-btn').addEventListener('click', () => {
      const silent = card.querySelector('.silent-check')?.checked ?? getSilentDefault();
      openLog('install', pkg.id, silent);
    });
  }
  return card;
}

// ── Installed packages ───────────────────────────────────────────────────────

const installedBody   = document.getElementById('installed-body');
const installedStatus = document.getElementById('installed-status');
const refreshBtn      = document.getElementById('refresh-btn');
const updateAllBtn    = document.getElementById('update-all-btn');
const installedFilter = document.getElementById('installed-filter');
const showArpToggle   = document.getElementById('show-arp-toggle');

let allInstalled = [];
let autoUpdatePrefs = {}; // { [packageId]: true } — only enabled entries are stored
let sortCol = null;   // null = default (updates first, then alpha)
let sortDir = 1;      // 1 = asc, -1 = desc

refreshBtn.addEventListener('click', loadInstalled);
updateAllBtn.addEventListener('click', () => {
  const packages = allInstalled.filter(p => p.hasUpdate && !p.isArp).map(p => p.id);
  if (packages.length === 0) return;
  openQueue(packages, getSilentDefault());
});
installedFilter.addEventListener('input', renderInstalled);
showArpToggle.addEventListener('change', renderInstalled);

// Column sort — clicking cycles: asc → desc → default
document.querySelectorAll('.installed-table th[data-sort]').forEach(th => {
  th.addEventListener('click', () => {
    const col = th.dataset.sort;
    if (sortCol === col) {
      if (sortDir === 1) { sortDir = -1; }
      else { sortCol = null; sortDir = 1; }   // third click resets
    } else {
      sortCol = col;
      sortDir = 1;
    }
    renderInstalled();
  });
});

async function loadInstalled() {
  refreshBtn.disabled = true;
  updateAllBtn.classList.add('hidden');
  installedStatus.textContent = 'Loading…';
  installedFilter.value = '';
  showInstalledSkeletons(12);

  try {
    const [installed, rawPrefs] = await Promise.all([
      invoke('list_installed'),
      invoke('get_auto_update_prefs').catch(() => ({})),
    ]);
    allInstalled = installed;
    autoUpdatePrefs = rawPrefs;
    renderInstalled();
  } catch (err) {
    installedBody.innerHTML = '';
    installedStatus.textContent = `Error: ${err}`;
  } finally {
    refreshBtn.disabled = false;
  }
}

function showInstalledSkeletons(n) {
  installedBody.innerHTML = '';
  for (let i = 0; i < n; i++) {
    const row = document.createElement('tr');
    row.className = 'skel-row';
    row.innerHTML = `
      <td>
        <div class="skel skel-name" style="width:${55 + (i % 4) * 10}%"></div>
        <div class="skel skel-id"   style="width:${35 + (i % 3) * 8}%"></div>
      </td>
      <td><div class="skel" style="height:13px;width:56px;border-radius:3px"></div></td>
      <td><div class="skel" style="height:20px;width:72px;border-radius:20px"></div></td>
      <td></td>
      <td></td>
    `;
    installedBody.appendChild(row);
  }
}

function renderInstalled() {
  const showArp    = showArpToggle.checked;
  const filterText = installedFilter.value.trim().toLowerCase();

  let packages = showArp ? allInstalled : allInstalled.filter(pkg => !pkg.isArp);

  if (filterText) {
    packages = packages.filter(pkg =>
      pkg.name.toLowerCase().includes(filterText) ||
      pkg.id.toLowerCase().includes(filterText)
    );
  }

  // Sort — respect active column or fall back to updates-first, alpha
  packages = [...packages].sort((a, b) => {
    if (sortCol === 'name')    return sortDir * a.name.localeCompare(b.name);
    if (sortCol === 'version') return sortDir * (a.version || '').localeCompare(b.version || '');
    if (sortCol === 'status') {
      if (a.hasUpdate !== b.hasUpdate) return a.hasUpdate ? -sortDir : sortDir;
      return a.name.localeCompare(b.name);
    }
    if (a.hasUpdate !== b.hasUpdate) return a.hasUpdate ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  // Update header indicators
  document.querySelectorAll('.installed-table th[data-sort]').forEach(th => {
    const active = th.dataset.sort === sortCol;
    th.classList.toggle('sort-active', active);
    th.querySelector('.sort-ind').textContent = active ? (sortDir === 1 ? '↑' : '↓') : '';
  });

  const total   = packages.length;
  const updates = packages.filter(p => p.hasUpdate).length;

  installedStatus.textContent = total === 0 && filterText
    ? 'No packages match your filter.'
    : `${total} package${total === 1 ? '' : 's'}${updates ? ` · ${updates} update${updates === 1 ? '' : 's'} available` : ''}`;

  // Show/hide "Update All" when there is ≥1 pending update and no filter active
  if (updates >= 1 && !filterText) {
    updateAllBtn.textContent = `Update All (${updates})`;
    updateAllBtn.classList.remove('hidden');
  } else {
    updateAllBtn.classList.add('hidden');
  }

  installedBody.innerHTML = '';
  packages.forEach(pkg => {
    const row = document.createElement('tr');
    const versionHtml = pkg.version
      ? `<span class="pkg-version">${escHtml(pkg.version)}</span>`
      : '<span class="pkg-version muted">—</span>';
    const availableHtml = pkg.hasUpdate && pkg.available
      ? `<span class="version-arrow">→ ${escHtml(pkg.available)}</span>`
      : '';

    // Non-ARP packages get an Uninstall button; ARP entries are registry
    // artifacts that winget/brew cannot manage so we omit it for them.
    const uninstallHtml = !pkg.isArp
      ? `<button class="btn-uninstall" data-pkg="${escHtml(pkg.id)}" data-name="${escHtml(pkg.name)}">🗑 Uninstall</button>`
      : '';

    const autoOn = !!autoUpdatePrefs[pkg.id];
    row.innerHTML = `
      <td>
        <div class="pkg-name">${escHtml(pkg.name)}</div>
        <div class="pkg-id">${escHtml(pkg.id)}</div>
      </td>
      <td class="version-cell">${versionHtml}${availableHtml}</td>
      <td>${
        pkg.hasUpdate
          ? '<span class="badge badge-update">⚠ Update available</span>'
          : '<span class="badge badge-ok">✓ Up to date</span>'
      }</td>
      <td>
        <div class="row-actions">
          ${pkg.hasUpdate ? `
            <label class="silent-label">
              <input type="checkbox" class="silent-check"${getSilentDefault() ? ' checked' : ''}>
              Silent
            </label>
            <button class="btn-update" data-pkg="${escHtml(pkg.id)}">Update</button>
          ` : ''}
          ${uninstallHtml}
        </div>
      </td>
      <td class="auto-update-cell">
        ${!pkg.isArp ? `<label class="toggle-label auto-update-label">
          <input type="checkbox" class="auto-update-check" data-pkg="${escHtml(pkg.id)}"${autoOn ? ' checked' : ''}>
          Auto-update
        </label>` : ''}
      </td>
    `;
    if (autoOn) row.classList.add('auto-update-on');
    installedBody.appendChild(row);
  });

  installedBody.querySelectorAll('.btn-update').forEach(btn => {
    btn.addEventListener('click', () => {
      const silent = btn.closest('.row-actions')?.querySelector('.silent-check')?.checked ?? getSilentDefault();
      openLog('update', btn.dataset.pkg, silent);
    });
  });

  installedBody.querySelectorAll('.btn-uninstall').forEach(btn => {
    btn.addEventListener('click', () => startUninstallConfirm(btn));
  });

  installedBody.querySelectorAll('.auto-update-check').forEach(cb => {
    cb.addEventListener('change', async () => {
      const pkgId  = cb.dataset.pkg;
      const enabled = cb.checked;
      // Optimistically update local state and row accent
      if (enabled) { autoUpdatePrefs[pkgId] = true; } else { delete autoUpdatePrefs[pkgId]; }
      cb.closest('tr').classList.toggle('auto-update-on', enabled);
      try {
        await invoke('set_auto_update_pref', { packageId: pkgId, enabled });
      } catch {
        // Revert on write failure
        cb.checked = !enabled;
        if (!enabled) { autoUpdatePrefs[pkgId] = true; } else { delete autoUpdatePrefs[pkgId]; }
        cb.closest('tr').classList.toggle('auto-update-on', !enabled);
      }
    });
  });
}

// ── Uninstall confirmation ────────────────────────────────────────────────────

// Track auto-cancel timer so we can clear it if the user acts first.
let uninstallCancelTimer = null;

function clearUninstallConfirm(btn, originalHtml) {
  clearTimeout(uninstallCancelTimer);
  uninstallCancelTimer = null;
  btn.outerHTML = originalHtml; // restore original button
}

function startUninstallConfirm(btn) {
  const pkgId   = btn.dataset.pkg;
  const pkgName = btn.dataset.name;
  const row     = btn.closest('tr');
  const silent  = row.querySelector('.silent-check')?.checked ?? getSilentDefault();

  // Replace the trash button with an inline confirmation strip.
  const originalHtml = btn.outerHTML;
  const strip = document.createElement('span');
  strip.className = 'uninstall-confirm';
  strip.innerHTML = `
    <span class="uninstall-confirm-label">Uninstall ${escHtml(pkgName)}?</span>
    <button class="btn-uninstall-yes">Yes, uninstall</button>
    <button class="btn-uninstall-cancel">Cancel</button>
  `;
  btn.replaceWith(strip);

  // Auto-cancel after 10 seconds
  uninstallCancelTimer = setTimeout(() => {
    const current = row.querySelector('.uninstall-confirm');
    if (current) current.outerHTML = originalHtml;
    uninstallCancelTimer = null;
  }, 10000);

  // Cancel button
  strip.querySelector('.btn-uninstall-cancel').addEventListener('click', e => {
    e.stopPropagation();
    clearTimeout(uninstallCancelTimer);
    strip.outerHTML = originalHtml;
  });

  // Confirm button
  strip.querySelector('.btn-uninstall-yes').addEventListener('click', e => {
    e.stopPropagation();
    clearTimeout(uninstallCancelTimer);
    openLog('uninstall', pkgId, silent);
  });

  // Click outside → cancel
  const outsideHandler = e => {
    if (!strip.contains(e.target)) {
      clearTimeout(uninstallCancelTimer);
      const current = row.querySelector('.uninstall-confirm');
      if (current) current.outerHTML = originalHtml;
      document.removeEventListener('click', outsideHandler);
    }
  };
  // defer one tick so the current click that opened the confirm doesn't
  // immediately trigger the outside handler
  setTimeout(() => document.addEventListener('click', outsideHandler), 0);
}

// ── Log modal ────────────────────────────────────────────────────────────────

const logBackdrop = document.getElementById('log-backdrop');
const logTitle    = document.getElementById('log-title');
const logOutput   = document.getElementById('log-output');
const logClose    = document.getElementById('log-close');
const logFooter   = document.getElementById('log-footer');

let unlistenOutput   = null;
let unlistenComplete = null;

logClose.addEventListener('click', closeLog);

async function openLog(action, pkgName, silent = getSilentDefault()) {
  logOutput.textContent = '';
  logFooter.innerHTML = '<div class="spinner"></div> <span>Running…</span>';
  logTitle.textContent =
    action === 'update'     ? `Updating: ${pkgName}`     :
    action === 'uninstall'  ? `Uninstalling: ${pkgName}` :
                              `Installing: ${pkgName}`;
  logClose.disabled = true;
  logBackdrop.classList.remove('hidden');

  // Listen for streaming output
  unlistenOutput = await listen('install-output', event => {
    logOutput.textContent += event.payload + '\n';
    logOutput.scrollTop = logOutput.scrollHeight;
  });

  // Listen for completion
  unlistenComplete = await listen('install-complete', event => {
    cleanup();
    logClose.disabled = false;

    if (event.payload === 'success') {
      logFooter.innerHTML = action === 'uninstall'
        ? '<span style="color: var(--success)">✓ Uninstalled!</span>'
        : '<span style="color: var(--success)">✓ Done!</span>';
      // Remove stale auto-update entry so the JSON doesn't accumulate dead entries
      if (action === 'uninstall') {
        invoke('set_auto_update_pref', { packageId: pkgName, enabled: false }).catch(() => {});
      }
      // Refresh installed list after any operation that changes it
      if (action === 'uninstall' ||
          document.getElementById('tab-installed').classList.contains('active')) {
        loadInstalled();
      }
    } else if (event.payload === 'app-running') {
      const msg = appRunningMessage(pkgName, /* short */ false);
      logFooter.innerHTML = `
        <span style="color:var(--warning)">⚠ ${escHtml(msg)}</span>
        <button class="btn-retry" id="log-retry-btn">Retry</button>
      `;
      document.getElementById('log-retry-btn').addEventListener('click', () => {
        openLog(action, pkgName, silent);
      });
    } else {
      logFooter.innerHTML = '<span style="color: var(--error)">✕ Something went wrong. See log above.</span>';
    }
  });

  // Kick off the Rust command
  try {
    if (action === 'update') {
      await invoke('update_package', { package: pkgName, silent });
    } else if (action === 'uninstall') {
      await invoke('uninstall_package', { package: pkgName, silent });
    } else {
      await invoke('install_package', { package: pkgName, silent });
    }
  } catch (err) {
    cleanup();
    logClose.disabled = false;
    logFooter.innerHTML = `<span style="color: var(--error)">Error: ${escHtml(String(err))}</span>`;
  }
}

function cleanup() {
  if (unlistenOutput)   { unlistenOutput();   unlistenOutput   = null; }
  if (unlistenComplete) { unlistenComplete();  unlistenComplete = null; }
}

function closeLog() {
  cleanup();
  logBackdrop.classList.add('hidden');
}

// ── Update-All Queue Panel ────────────────────────────────────────────────────

const queuePanel    = document.getElementById('queue-panel');
const queueList     = document.getElementById('queue-list');
const queueLog      = document.getElementById('queue-log');
const queueProgressEl = document.getElementById('queue-progress');
const abortBtn      = document.getElementById('abort-btn');

let queueUnlistenOutput   = null;
let queueUnlistenStart    = null;
let queueUnlistenDone     = null;
let queueUnlistenComplete = null;
let queueDone = false;
let currentPkgLog = []; // collects output lines for the package currently running

abortBtn.addEventListener('click', () => {
  if (queueDone) {
    closeQueue();
    loadInstalled();
  } else {
    invoke('abort_update_all');
    abortBtn.textContent = 'Aborting…';
    abortBtn.disabled = true;
  }
});

function findQueueItem(name) {
  for (const li of queueList.querySelectorAll('.queue-item')) {
    if (li.dataset.pkg === name) return li;
  }
  return null;
}

function cleanupQueue() {
  if (queueUnlistenOutput)   { queueUnlistenOutput();   queueUnlistenOutput   = null; }
  if (queueUnlistenStart)    { queueUnlistenStart();    queueUnlistenStart    = null; }
  if (queueUnlistenDone)     { queueUnlistenDone();     queueUnlistenDone     = null; }
  if (queueUnlistenComplete) { queueUnlistenComplete(); queueUnlistenComplete = null; }
  currentPkgLog = [];
}

function closeQueue() {
  cleanupQueue();
  queuePanel.classList.add('hidden');
}

async function openQueue(packages, silent) {
  queueDone = false;
  queueList.innerHTML = '';
  queueLog.textContent = '';
  queueProgressEl.textContent = `0 of ${packages.length}`;
  abortBtn.textContent = 'Abort';
  abortBtn.disabled = false;
  abortBtn.classList.remove('done');

  // Build the queue item list
  packages.forEach(pkg => {
    const li = document.createElement('li');
    li.className = 'queue-item queue-pending';
    li.dataset.pkg = pkg;
    li.innerHTML = `
      <span class="qi-icon"></span>
      <div class="qi-info">
        <span class="qi-name" title="${escHtml(pkg)}">${escHtml(pkg)}</span>
      </div>
      <button class="qi-skip">Skip</button>
    `;
    li.querySelector('.qi-skip').addEventListener('click', async () => {
      if (!li.classList.contains('queue-pending')) return;
      li.classList.replace('queue-pending', 'queue-skipped');
      li.querySelector('.qi-skip').remove();
      await invoke('skip_package', { pkg });
    });
    queueList.appendChild(li);
  });

  queuePanel.classList.remove('hidden');

  // Wire up event listeners
  queueUnlistenOutput = await listen('install-output', e => {
    currentPkgLog.push(e.payload);
    queueLog.textContent += e.payload + '\n';
    queueLog.scrollTop = queueLog.scrollHeight;
  });

  queueUnlistenStart = await listen('pkg-start', e => {
    currentPkgLog = [];   // fresh log buffer for the new package
    const { name, index, total } = e.payload;
    queueProgressEl.textContent = `${index} of ${total}`;
    const li = findQueueItem(name);
    if (!li) return;
    li.classList.replace('queue-pending', 'queue-active');
    li.querySelector('.qi-skip')?.remove();
    li.querySelector('.qi-icon').innerHTML = '<span class="qi-spinner"></span>';
    li.scrollIntoView({ block: 'nearest' });
  });

  queueUnlistenDone = await listen('pkg-done', e => {
    const { name, status } = e.payload;
    const li = findQueueItem(name);
    if (!li) return;
    li.classList.remove('queue-active', 'queue-pending');
    li.classList.add(`queue-${status}`);
    li.querySelector('.qi-icon').innerHTML = ''; // spinner → state icon via CSS ::before

    if (status === 'error') {
      // Show the last meaningful output line as a hint so the user can see why it failed
      // without having to scroll the full log. Full output is always in the log pane below.
      const hint = [...currentPkgLog].reverse().find(l => l.trim().length > 4) ?? '';
      if (hint) {
        const el = document.createElement('span');
        el.className = 'qi-error-hint';
        el.title = hint;           // full text on hover
        el.textContent = hint;     // CSS truncates with ellipsis
        li.querySelector('.qi-info').appendChild(el);
      }
    } else if (status === 'app-running') {
      const hint = document.createElement('span');
      hint.className = 'qi-app-running-hint';
      hint.textContent = appRunningMessage(name, /* short */ true);
      li.querySelector('.qi-info').appendChild(hint);

      const retryBtn = document.createElement('button');
      retryBtn.className = 'qi-retry';
      retryBtn.textContent = 'Retry';
      retryBtn.addEventListener('click', () => openLog('update', name, silent));
      li.appendChild(retryBtn);
    }
  });

  queueUnlistenComplete = await listen('install-complete', () => {
    cleanupQueue();
    queueDone = true;
    abortBtn.textContent = 'Close';
    abortBtn.disabled = false;
    abortBtn.classList.add('done');
  });

  // Start the Rust command
  try {
    await invoke('update_all_packages_queued', { packages, silent });
  } catch (err) {
    cleanupQueue();
    queueLog.textContent += `\nError: ${err}\n`;
    queueDone = true;
    abortBtn.textContent = 'Close';
    abortBtn.disabled = false;
    abortBtn.classList.add('done');
  }
}

// ── Keyboard shortcuts ───────────────────────────────────────────────────────

document.addEventListener('keydown', e => {
  // Escape: close modal if done, dismiss uninstall confirmation, or clear filter
  if (e.key === 'Escape') {
    if (!logBackdrop.classList.contains('hidden')) {
      if (!logClose.disabled) closeLog();
      return;
    }
    // Dismiss any open uninstall confirmation strip
    const confirm = document.querySelector('.uninstall-confirm');
    if (confirm) {
      clearTimeout(uninstallCancelTimer);
      // Restore the original trash button by re-rendering the installed list.
      // Simpler than tracking originalHtml across closures.
      renderInstalled();
      return;
    }
    if (installedFilter.value) {
      installedFilter.value = '';
      renderInstalled();
      return;
    }
  }

  // F5 or Ctrl+R while on the installed tab → refresh
  if ((e.key === 'F5' || (e.ctrlKey && e.key === 'r')) &&
      document.getElementById('tab-installed').classList.contains('active')) {
    e.preventDefault();
    loadInstalled();
  }
});

// Auto-focus the welcome search field on launch
searchInputWelcome.focus();

// ── Helpers ──────────────────────────────────────────────────────────────────

// Per-app hints for the "app is running" failure state.
// Keys are lowercase substrings of the package ID.
const APP_RUNNING_HINTS = {
  'microsoft.visualstudiocode': {
    long:  'VS Code has background processes (language servers, extensions host). Close all VS Code windows, then check Task Manager for any remaining Code.exe processes.',
    short: 'Close all VS Code windows + check Task Manager for Code.exe',
  },
  'google.chrome': {
    long:  'Chrome runs in the background even after closing windows. Right-click the Chrome icon in the system tray and choose Exit, or end all Chrome.exe processes in Task Manager.',
    short: 'Exit Chrome from the system tray or kill Chrome.exe in Task Manager',
  },
  'slacktechnologies.slack': {
    long:  'Slack runs in the background. Right-click its system tray icon and choose Quit Slack, or end Slack.exe in Task Manager.',
    short: 'Quit Slack from the system tray or kill Slack.exe in Task Manager',
  },
  'microsoft.teams': {
    long:  'Teams runs in the background. Right-click its system tray icon and choose Quit, or end all Teams.exe processes in Task Manager.',
    short: 'Quit Teams from the system tray or kill Teams.exe in Task Manager',
  },
  'discord.discord': {
    long:  'Discord runs in the background. Right-click its system tray icon and choose Quit Discord, or end Discord.exe in Task Manager.',
    short: 'Quit Discord from the system tray or kill Discord.exe in Task Manager',
  },
  'spotify.spotify': {
    long:  'Spotify runs in the background. Right-click its system tray icon and choose Quit Spotify, or end Spotify.exe in Task Manager.',
    short: 'Quit Spotify from the system tray or kill Spotify.exe in Task Manager',
  },
};

function appRunningMessage(pkgId, short) {
  if (/brewinget/i.test(pkgId)) {
    return short
      ? 'Restart Brewinget to update itself'
      : 'Brewinget needs to be closed to update itself. Restart from a fresh launch and try again.';
  }
  const lower = pkgId.toLowerCase();
  for (const [key, msgs] of Object.entries(APP_RUNNING_HINTS)) {
    if (lower.includes(key)) return short ? msgs.short : msgs.long;
  }
  return short
    ? 'Close all instances (check Task Manager for background processes)'
    : 'The app or its background processes are still running. Close all instances and try again. Tip: check Task Manager — background processes like update helpers or language servers often survive window close.';
}

function escHtml(str) {
  return str.replace(/[&<>"']/g, c => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[c]);
}
