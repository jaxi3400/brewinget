// Tauri injects these globals because we set "withGlobalTauri": true in tauri.conf.json
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

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

const searchInput = document.getElementById('search-input');
const searchBtn   = document.getElementById('search-btn');
const searchStatus = document.getElementById('search-status');
const resultsGrid  = document.getElementById('results-grid');

searchBtn.addEventListener('click', doSearch);
searchInput.addEventListener('keydown', e => { if (e.key === 'Enter') doSearch(); });

async function doSearch() {
  const query = searchInput.value.trim();
  if (!query) return;

  searchBtn.disabled = true;
  searchStatus.textContent = 'Searching…';
  resultsGrid.innerHTML = '';

  try {
    const packages = await invoke('search_packages', { query });

    if (packages.length === 0) {
      searchStatus.textContent = 'No packages found.';
    } else {
      searchStatus.textContent = `${packages.length} result${packages.length === 1 ? '' : 's'} for "${query}"`;
      packages.forEach(pkg => {
        resultsGrid.appendChild(makeCard(pkg));
      });
    }
  } catch (err) {
    searchStatus.textContent = `Error: ${err}`;
  } finally {
    searchBtn.disabled = false;
  }
}

function makeCard(pkg) {
  const card = document.createElement('div');
  card.className = 'pkg-card';
  card.innerHTML = `
    <div class="pkg-name">${escHtml(pkg.name)}</div>
    <div class="pkg-id">${escHtml(pkg.id)}</div>
    ${pkg.version ? `<div class="pkg-meta">${escHtml(pkg.version)}${pkg.source ? ` · ${escHtml(pkg.source)}` : ''}</div>` : ''}
    <button class="install-btn">Install</button>
  `;
  card.querySelector('.install-btn').addEventListener('click', () => openLog('install', pkg.id));
  return card;
}

// ── Installed packages ───────────────────────────────────────────────────────

const installedBody   = document.getElementById('installed-body');
const installedStatus = document.getElementById('installed-status');
const refreshBtn      = document.getElementById('refresh-btn');
const installedFilter = document.getElementById('installed-filter');
const showArpToggle   = document.getElementById('show-arp-toggle');

let allInstalled = [];

refreshBtn.addEventListener('click', loadInstalled);
installedFilter.addEventListener('input', renderInstalled);
showArpToggle.addEventListener('change', renderInstalled);

async function loadInstalled() {
  refreshBtn.disabled = true;
  installedStatus.textContent = 'Loading…';
  installedBody.innerHTML = '';
  installedFilter.value = '';

  try {
    allInstalled = await invoke('list_installed');
    renderInstalled();
  } catch (err) {
    installedStatus.textContent = `Error: ${err}`;
  } finally {
    refreshBtn.disabled = false;
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

  // Updates at top, then alphabetical by name
  packages = [...packages].sort((a, b) => {
    if (a.hasUpdate !== b.hasUpdate) return a.hasUpdate ? -1 : 1;
    return a.name.localeCompare(b.name);
  });

  const total   = packages.length;
  const updates = packages.filter(p => p.hasUpdate).length;
  installedStatus.textContent = total === 0 && filterText
    ? 'No packages match your filter.'
    : `${total} package${total === 1 ? '' : 's'}${updates ? ` · ${updates} update${updates === 1 ? '' : 's'} available` : ''}`;

  installedBody.innerHTML = '';
  packages.forEach(pkg => {
    const row = document.createElement('tr');
    row.innerHTML = `
      <td>
        <div class="pkg-name">${escHtml(pkg.name)}</div>
        <div class="pkg-id">${escHtml(pkg.id)}</div>
      </td>
      <td>${
        pkg.hasUpdate
          ? '<span class="badge badge-update">⚠ Update available</span>'
          : '<span class="badge badge-ok">✓ Up to date</span>'
      }</td>
      <td>${
        pkg.hasUpdate
          ? `<button class="btn-update" data-pkg="${escHtml(pkg.id)}">Update</button>`
          : '—'
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
  logTitle.textContent = action === 'update' ? `Updating: ${pkgName}` : `Installing: ${pkgName}`;
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
    if (action === 'update') {
      await invoke('update_package', { package: pkgName });
    } else {
      await invoke('install_package', { package: pkgName });
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

// ── Helpers ──────────────────────────────────────────────────────────────────

function escHtml(str) {
  return str.replace(/[&<>"']/g, c => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[c]);
}
