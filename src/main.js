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

// ── Search ───────────────────────────────────────────────────────────────────

const searchInput       = document.getElementById('search-input');
const searchBtn         = document.getElementById('search-btn');
const searchStatus      = document.getElementById('search-status');
const searchToolbar     = document.getElementById('search-toolbar');
const searchEmpty       = document.getElementById('search-empty');
const resultsGrid       = document.getElementById('results-grid');
const showMsStoreToggle = document.getElementById('show-msstore-toggle');

let allSearchResults = [];
let lastQuery = '';

searchBtn.addEventListener('click', doSearch);
searchInput.addEventListener('keydown', e => { if (e.key === 'Enter') doSearch(); });
showMsStoreToggle.addEventListener('change', renderSearch);

function isMsStore(pkg) {
  return pkg.source === 'msstore' || /^[0-9A-Z]{9,13}$/.test(pkg.id);
}

async function doSearch() {
  const query = searchInput.value.trim();
  if (!query) return;

  lastQuery = query;
  searchBtn.disabled = true;
  searchEmpty.classList.add('hidden');
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
    searchBtn.disabled = false;
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

  card.innerHTML = `
    <div class="pkg-name" title="${escHtml(pkg.name)}">${escHtml(pkg.name)}</div>
    <div class="pkg-id"  title="${escHtml(pkg.id)}">${escHtml(pkg.id)}</div>
    <div class="pkg-footer">${versionHtml}${badgeHtml}</div>
    <button class="${btnClass}">${btnLabel}</button>
  `;
  if (!pkg.installed) {
    card.querySelector('.install-btn').addEventListener('click', () => openLog('install', pkg.id));
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
let sortCol = null;   // null = default (updates first, then alpha)
let sortDir = 1;      // 1 = asc, -1 = desc

refreshBtn.addEventListener('click', loadInstalled);
updateAllBtn.addEventListener('click', () => openLog('update-all', '--all'));
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
    allInstalled = await invoke('list_installed');
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

  // Show/hide "Update All" only when there are ≥2 pending updates and no filter active
  if (updates >= 2 && !filterText) {
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
      <td>${
        pkg.hasUpdate
          ? `<button class="btn-update" data-pkg="${escHtml(pkg.id)}">Update</button>`
          : ''
      }</td>
    `;
    installedBody.appendChild(row);
  });

  installedBody.querySelectorAll('.btn-update').forEach(btn => {
    btn.addEventListener('click', () => openLog('update', btn.dataset.pkg));
  });
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

async function openLog(action, pkgName) {
  logOutput.textContent = '';
  logFooter.innerHTML = '<div class="spinner"></div> <span>Running…</span>';
  logTitle.textContent = action === 'update-all' ? 'Updating all packages…'
    : action === 'update' ? `Updating: ${pkgName}`
    : `Installing: ${pkgName}`;
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
      logFooter.innerHTML = '<span style="color: var(--success)">✓ Done!</span>';
      // Refresh installed list if we're on that tab
      if (document.getElementById('tab-installed').classList.contains('active')) {
        loadInstalled();
      }
    } else {
      logFooter.innerHTML = '<span style="color: var(--error)">✕ Something went wrong. See log above.</span>';
    }
  });

  // Kick off the Rust command
  try {
    if (action === 'update-all') {
      await invoke('update_all_packages');
    } else if (action === 'update') {
      await invoke('update_package', { package: pkgName, silent: false });
    } else {
      await invoke('install_package', { package: pkgName, silent: false });
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

// ── Keyboard shortcuts ───────────────────────────────────────────────────────

document.addEventListener('keydown', e => {
  // Escape: close modal if done, or clear the installed filter
  if (e.key === 'Escape') {
    if (!logBackdrop.classList.contains('hidden')) {
      if (!logClose.disabled) closeLog();
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

// Auto-focus the search field on launch
searchInput.focus();

// ── Helpers ──────────────────────────────────────────────────────────────────

function escHtml(str) {
  return str.replace(/[&<>"']/g, c => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[c]);
}
