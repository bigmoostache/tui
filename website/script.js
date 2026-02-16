// ===== Global State =====
const state = {
    currentPanel: 'welcome',
    userXP: 0,
    userLevel: 1,
    achievements: new Set(),
    unlockedPanels: new Set(['welcome']),
    bootComplete: false,
    commandHistory: [],
    historyIndex: -1,
    combo: 0,
    comboTimer: null,
    lastCommandTime: 0,
    highScore: parseInt(localStorage.getItem('contextPilotHighScore') || '0'),
    theme: localStorage.getItem('contextPilotTheme') || 'green',
    konamiCode: [],
    konamiSequence: ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a']
};

// ===== Achievement Definitions =====
const achievements = {
    'first_boot': { name: 'System Initialize', xp: 50, desc: 'Completed boot sequence' },
    'help_seeker': { name: 'RTFM', xp: 25, desc: 'Read the help menu' },
    'explorer': { name: 'Panel Explorer', xp: 100, desc: 'Unlocked all panels' },
    'feature_reader': { name: 'Feature Complete', xp: 75, desc: 'Read all features' },
    'demo_master': { name: 'Demo Master', xp: 150, desc: 'Completed interactive demo' },
    'speedrunner': { name: 'Speedrunner', xp: 200, desc: 'Reached install screen quickly' },
    'command_master': { name: 'Command Master', xp: 100, desc: 'Used 10 different commands' },
    'level_up': { name: 'Level Up', xp: 0, desc: 'Reached level 2' },
    'completionist': { name: 'Completionist', xp: 500, desc: 'Unlocked all achievements' },
    'readme': { name: 'Documentation Reader', xp: 50, desc: 'Checked the docs' }
};

// ===== Content Definitions =====
const panelContent = {
    welcome: {
        title: 'WELCOME TO CONTEXT PILOT',
        content: `
<div class="ascii-art">
╔══════════════════════════════════════════════════════════════════╗
║   ___  ___  _  _ _____ _____  __ _____ _                        ║
║  / __|/ _ \\| \\| |_   _| __\\ \\/ /_   _| |                        ║
║ | (__| (_) | .\` | | | | _| >  <  | | | |__                      ║
║  \\___|\\___/|_|\\_| |_| |___/_/\\_\\ |_| |____|                     ║
║                                                                  ║
║  ██████╗ ██╗██╗      ██████╗ ████████╗                          ║
║  ██╔══██╗██║██║     ██╔═══██╗╚══██╔══╝                          ║
║  ██████╔╝██║██║     ██║   ██║   ██║                             ║
║  ██╔═══╝ ██║██║     ██║   ██║   ██║                             ║
║  ██║     ██║███████╗╚██████╔╝   ██║                             ║
║  ╚═╝     ╚═╝╚══════╝ ╚═════╝    ╚═╝                             ║
╚══════════════════════════════════════════════════════════════════╝
</div>

<div class="content-section">
    <div class="section-title">» SYSTEM STATUS: OPERATIONAL</div>
    <div class="section-subtitle">AI-Native Terminal IDE • Version 2.0.1 • Build 20260216</div>
    
    <p>Your AI coding assistant has <strong style="color: var(--crt-red)">amnesia</strong>. This one doesn't.</p>
    
    <p>Every AI coding tool has the same problem: <strong>context is invisible</strong>. You paste code, 
    the AI forgets it three messages later, you paste it again. Context Pilot makes context <strong>visible</strong>.</p>
    
    <div class="feature-box">
        <div class="feature-title">⎈ FULL CONTEXT CONTROL</div>
        Every piece of information — files, diffs, grep results, memories — is a <strong>panel</strong> 
        with a live token count. <span style="color: var(--crt-amber)">You</span> decide what stays.
    </div>
    
    <div class="feature-box">
        <div class="feature-title">⚡ TERMINAL-NATIVE SPEED</div>
        Built in Rust with ratatui. Sub-millisecond rendering. No Electron bloat. 
        Just raw terminal performance.
    </div>
    
    <div class="feature-box">
        <div class="feature-title">🧠 PERSISTENT MEMORY</div>
        Context survives across sessions. The AI remembers your preferences, conventions, 
        and ongoing tasks.
    </div>
</div>

<div class="content-section">
    <div class="section-title">» QUICK START</div>
    <p>Navigate using commands:</p>
    <div class="command-example">❯ next          # Go to next panel</div>
    <div class="command-example">❯ features      # View features</div>
    <div class="command-example">❯ demo          # Try interactive demo</div>
    <div class="command-example">❯ help          # Show all commands</div>
</div>

<div class="progress-bar">
    <div class="progress-label">
        <span>PROGRESS TO NEXT LEVEL</span>
        <span id="xp-progress">0 / 250 XP</span>
    </div>
    <div class="progress-track">
        <div class="progress-fill" id="progress-fill" style="width: 0%"></div>
    </div>
</div>
        `
    },
    
    features: {
        title: 'CORE FEATURES',
        content: `
<div class="content-section">
    <div class="section-title">» WHY CONTEXT PILOT?</div>
    <div class="section-subtitle">Every token matters. Surgical control over your AI's context window.</div>
</div>

<div class="feature-box">
    <div class="feature-title">🎯 CONTEXT CONTROL</div>
    <p>Open files, git diffs, grep results as context panels. See exactly what your AI sees — 
    with token counts. Close what you don't need.</p>
    <div class="command-example">❯ file_open src/main.rs    # Add file to context
❯ panel_close P7            # Remove from context</div>
</div>

<div class="feature-box">
    <div class="feature-title">⚡ BLAZING FAST</div>
    <p>Built in Rust with ratatui. Sub-millisecond rendering. Instant file operations. 
    No Electron, no web browser, no bloat.</p>
    <ul>
        <li>Sub-1ms TUI rendering with ratatui</li>
        <li>Instant file operations</li>
        <li>Real-time context updates</li>
        <li>Zero startup time</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🔌 MODULAR ARCHITECTURE</div>
    <p>Activate only what you need: Git, GitHub, files, tree, console, todos, memory.</p>
    <ul>
        <li><strong>Files:</strong> Open, edit, write with syntax awareness</li>
        <li><strong>Git:</strong> Status, diffs, log, blame as live panels</li>
        <li><strong>Tree:</strong> Directory navigation with filtering</li>
        <li><strong>Console:</strong> Monitor terminal output via tmux</li>
        <li><strong>Memory:</strong> Persistent notes across sessions</li>
        <li><strong>Todo:</strong> Task tracking and planning</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🤖 MULTI-PROVIDER</div>
    <p>Works with multiple AI providers:</p>
    <ul>
        <li>Anthropic Claude (3.5 Sonnet, Opus)</li>
        <li>OpenAI (GPT-4, GPT-4 Turbo)</li>
        <li>DeepSeek</li>
        <li>Grok</li>
        <li>Groq (fast inference)</li>
        <li>Any OpenAI-compatible API</li>
    </ul>
    <p>Switch providers mid-conversation. Compare outputs.</p>
</div>

<div class="feature-box">
    <div class="feature-title">📊 VISIBILITY INTO CONTEXT</div>
    <p>Know exactly what your AI sees:</p>
    <div class="command-example">┌─ Context Panels ────────────────────┐
│ P1 Chat       │ 250 tokens          │
│ P2 Tree       │ 1,884 tokens        │
│ P3 Todos      │ 316 tokens          │
│ P7 main.rs    │ 6,420 tokens        │
└─────────────────────────────────────┘
Context: 8,870 / 200,000 tokens (4.4%)</div>
</div>

<div class="content-section">
    <div class="section-title">» COMPARISON</div>
    <table class="data-table">
        <tr>
            <th></th>
            <th style="color: var(--crt-highlight)">Context Pilot</th>
            <th>Cursor</th>
            <th>Claude Code</th>
        </tr>
        <tr>
            <td>Context Control</td>
            <td style="color: var(--crt-green)">✓ You choose</td>
            <td>Automatic</td>
            <td>Automatic</td>
        </tr>
        <tr>
            <td>See AI's context</td>
            <td style="color: var(--crt-green)">✓ Token-level</td>
            <td>Partial</td>
            <td>✗</td>
        </tr>
        <tr>
            <td>Terminal Native</td>
            <td style="color: var(--crt-green)">✓ Pure TUI</td>
            <td>✗ Electron</td>
            <td>✓ CLI</td>
        </tr>
        <tr>
            <td>Multi-provider</td>
            <td style="color: var(--crt-green)">✓ 5+ providers</td>
            <td>✓</td>
            <td>✗ Claude only</td>
        </tr>
        <tr>
            <td>Open Source</td>
            <td style="color: var(--crt-green)">✓ GPL-3.0</td>
            <td>✗</td>
            <td>✗</td>
        </tr>
        <tr>
            <td>Cost</td>
            <td style="color: var(--crt-green)">Free (BYO API key)</td>
            <td>$20/mo+</td>
            <td>$20/mo+</td>
        </tr>
    </table>
</div>
        `
    },
    
    modules: {
        title: 'MODULE SYSTEM',
        content: `
<div class="content-section">
    <div class="section-title">» MODULAR BY DESIGN</div>
    <div class="section-subtitle">Activate what you need. Each module is independent.</div>
</div>

<div class="feature-box">
    <div class="feature-title">📁 FILES MODULE</div>
    <p>Open, edit, and write files. Full file context with syntax awareness.</p>
    <div class="command-example">Tools: file_open, file_edit, file_write, file_search_replace</div>
    <ul>
        <li>Opens files as context panels</li>
        <li>Precise search-and-replace editing</li>
        <li>Syntax-aware formatting</li>
        <li>Batch file operations</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🌳 TREE MODULE</div>
    <p>Directory tree with gitignore-style filtering. Annotate files with descriptions.</p>
    <div class="command-example">Tools: tree_filter, tree_toggle, tree_describe</div>
    <ul>
        <li>Gitignore-style pattern filtering</li>
        <li>Collapsible directory structure</li>
        <li>File annotations and notes</li>
        <li>Live context panel</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🔀 GIT MODULE</div>
    <p>Full git integration. Status, diffs, log, blame — all as context panels.</p>
    <div class="command-example">Tools: git_execute, git_configure_p6, git_status, git_diff</div>
    <ul>
        <li>Live git status updates</li>
        <li>Diff visualization in context</li>
        <li>Git log and blame</li>
        <li>Configurable auto-refresh</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🐙 GITHUB MODULE</div>
    <p>GitHub CLI integration. View PRs, issues, create releases.</p>
    <div class="command-example">Tools: gh_execute</div>
    <ul>
        <li>PR and issue management</li>
        <li>Release creation</li>
        <li>CI/CD status checks</li>
        <li>Direct GitHub API access</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🖥️ CONSOLE MODULE (TMUX)</div>
    <p>Monitor terminal output in real-time. Run builds, tests, servers.</p>
    <div class="command-example">Tools: console_create, console_send_keys, console_capture</div>
    <ul>
        <li>Live terminal output monitoring</li>
        <li>AI sees what you see</li>
        <li>Multi-pane tmux integration</li>
        <li>Build/test automation</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">✅ TODO MODULE</div>
    <p>Task tracking with nested todos. Plan work, track progress.</p>
    <div class="command-example">Tools: todo_create, todo_update, todo_complete</div>
    <ul>
        <li>Hierarchical task structure</li>
        <li>Persistent across sessions</li>
        <li>AI-driven task completion</li>
        <li>Progress tracking</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🧠 MEMORY MODULE</div>
    <p>Persistent memories across conversations. Store preferences and conventions.</p>
    <div class="command-example">Tools: memory_create, memory_update, memory_search</div>
    <ul>
        <li>Long-term memory storage</li>
        <li>Project-specific context</li>
        <li>Convention tracking</li>
        <li>Searchable memory bank</li>
    </ul>
</div>

<div class="feature-box">
    <div class="feature-title">🦴 SPINE MODULE</div>
    <p>Central nervous system. Manages notifications and auto-continuation.</p>
    <div class="command-example">Tools: spine_configure, notification_create, auto_continue</div>
    <ul>
        <li>Guard rails (max tokens, cost limits)</li>
        <li>Auto-continuation for long tasks</li>
        <li>Notification system</li>
        <li>Stream control</li>
    </ul>
</div>
        `
    },
    
    demo: {
        title: 'INTERACTIVE DEMO',
        content: `
<div class="content-section">
    <div class="section-title">» LIVE DEMO SIMULATION</div>
    <div class="section-subtitle">Experience Context Pilot in action</div>
</div>

<div class="ascii-art">
┌────────────────────────────────────────────────────────────────┐
│ DEMO: Refactoring with Full Context Visibility                │
└────────────────────────────────────────────────────────────────┘
</div>

<div class="command-example" style="margin: 20px 0;">
<span style="color: var(--crt-text-dim);">// Initial state - user opens relevant files</span>
❯ file_open src/auth/mod.rs
<span style="color: var(--crt-green);">✓</span> Opened as <span style="color: var(--crt-cyan);">P7</span> (2,450 tokens)

❯ file_open src/auth/provider.rs  
<span style="color: var(--crt-green);">✓</span> Opened as <span style="color: var(--crt-cyan);">P8</span> (1,890 tokens)

❯ git_status
<span style="color: var(--crt-green);">✓</span> Git status updated in <span style="color: var(--crt-cyan);">P6</span>

<span style="color: var(--crt-text-dim);">┌─ Context Status ───────────────────┐
│ Total: 6,420 / 200,000 (3.2%)     │
│ P1 Chat       │ 350 tokens        │
│ P2 Tree       │ 1,100 tokens      │
│ P6 Git        │ 630 tokens        │
│ P7 mod.rs     │ 2,450 tokens      │
│ P8 provider.rs│ 1,890 tokens      │
└────────────────────────────────────┘</span>

<span style="color: var(--crt-text-dim);">// User asks AI to refactor</span>
❯ Refactor auth module to use async traits

<span style="color: var(--crt-green);">⚙</span> Analyzing auth module structure...
<span style="color: var(--crt-green);">⚙</span> Found 3 sync trait methods to convert
<span style="color: var(--crt-green);">⚙</span> Identifying call sites across codebase...

<span style="color: var(--crt-green);">✓</span> Replaced trait definitions in mod.rs
<span style="color: var(--crt-green);">✓</span> Updated provider implementations
<span style="color: var(--crt-green);">✓</span> Modified 12 call sites across 4 files
<span style="color: var(--crt-green);">✓</span> All tests passing

<span style="color: var(--crt-text-dim);">// Context automatically updated</span>
<span style="color: var(--crt-cyan);">P6 Git</span> status changed - staged changes detected
<span style="color: var(--crt-cyan);">P7 mod.rs</span> content updated (2,580 tokens +130)
<span style="color: var(--crt-cyan);">P8 provider.rs</span> content updated (1,950 tokens +60)

<span style="color: var(--crt-amber);">📊 Context: 6,610 / 200,000 (3.3%)</span>
</div>

<div class="feature-box">
    <div class="feature-title">🎯 KEY INSIGHTS</div>
    <ul>
        <li><strong>Full Visibility:</strong> You see exactly what the AI sees - every file, every token</li>
        <li><strong>Context Control:</strong> Choose which files are in context before asking</li>
        <li><strong>Live Updates:</strong> Git status and file contents update in real-time</li>
        <li><strong>Efficient:</strong> Only 3.3% of context used for complete refactoring</li>
        <li><strong>Transparent:</strong> AI actions and context changes clearly logged</li>
    </ul>
</div>

<div class="content-section">
    <div class="section-title">» REAL RESULTS</div>
    <p>In production usage:</p>
    <div class="feature-box">
        <div class="feature-title">📈 90+ FILES EXPLORED</div>
        <p>Single session ended at <strong style="color: var(--crt-highlight)">14% context usage</strong>. 
        The AI read everything, understood it, annotated it, and freed the space.</p>
    </div>
    <div class="feature-box">
        <div class="feature-title">🧠 CONTEXT MANAGEMENT</div>
        <p>AI actively manages its own context. Opens what it needs, closes what it doesn't, 
        takes notes for later. <strong>Because it can see what's in its context.</strong></p>
    </div>
</div>
        `
    },
    
    install: {
        title: 'INSTALLATION',
        content: `
<div class="content-section">
    <div class="section-title">» GETTING STARTED</div>
    <div class="section-subtitle">Up and running in minutes</div>
</div>

<div class="feature-box">
    <div class="feature-title">STEP 1: CLONE & BUILD</div>
    <div class="command-example">$ git clone https://github.com/bigmoostache/context-pilot.git
$ cd context-pilot
$ cargo build --release</div>
    <p style="margin-top: 10px;">Binary will be at: <code class="code-inline">target/release/context-pilot</code></p>
</div>

<div class="feature-box">
    <div class="feature-title">STEP 2: CONFIGURE API KEY</div>
    <div class="command-example"># Create .env in your project directory
$ echo 'ANTHROPIC_API_KEY=sk-ant-...' > .env

# Or export directly
$ export ANTHROPIC_API_KEY=sk-ant-...</div>
    <p style="margin-top: 10px;">Supported providers: Anthropic, OpenAI, DeepSeek, Grok, Groq</p>
</div>

<div class="feature-box">
    <div class="feature-title">STEP 3: LAUNCH</div>
    <div class="command-example"># Navigate to your project
$ cd your-project

# Launch Context Pilot (requires tmux)
$ tmux new-session context-pilot</div>
    <p style="margin-top: 10px;">That's it! You're ready to code with full context control.</p>
</div>

<div class="content-section">
    <div class="section-title">» REQUIREMENTS</div>
    <ul>
        <li><strong>Rust toolchain</strong> (for building from source)</li>
        <li><strong>tmux</strong> (for console integration)</li>
        <li><strong>API key</strong> from supported provider (Anthropic, OpenAI, etc.)</li>
        <li><strong>Git</strong> (optional, for git module)</li>
        <li><strong>GitHub CLI</strong> (optional, for GitHub module)</li>
    </ul>
</div>

<div class="content-section">
    <div class="section-title">» QUICK START COMMANDS</div>
    <div class="command-example">Ctrl+P           # Command palette
Ctrl+N           # Next context panel
↑ ↓              # Scroll active panel
Enter            # Send message
Ctrl+L           # Clear conversation
p1-p99           # Jump to panel
/cmd             # Run command
Ctrl+Q           # Quit</div>
</div>

<div class="content-section">
    <div class="section-title">» LEARN MORE</div>
    <ul>
        <li><strong>GitHub:</strong> <a href="https://github.com/bigmoostache/context-pilot" style="color: var(--crt-cyan)">github.com/bigmoostache/context-pilot</a></li>
        <li><strong>Documentation:</strong> Check /docs folder in repo</li>
        <li><strong>Issues:</strong> Report bugs or request features on GitHub</li>
        <li><strong>Contributing:</strong> PRs welcome! See CONTRIBUTING.md</li>
    </ul>
</div>

<div class="ascii-art" style="color: var(--crt-highlight); margin: 30px 0;">
╔══════════════════════════════════════════════════════════════╗
║                                                              ║
║   🚀 READY TO PILOT YOUR CONTEXT?                           ║
║                                                              ║
║   Build it. Configure it. Launch it.                        ║
║   Take control of your AI's context.                        ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
</div>
        `
    }
};

// ===== Boot Sequence =====
function runBootSequence() {
    const bootText = document.getElementById('boot-text');
    const bootSeq = document.getElementById('boot-sequence');
    const mainInterface = document.getElementById('main-interface');
    
    const bootLines = [
        '> INITIALIZING CONTEXT-PILOT v2.0.1...',
        '',
        '[ OK ] Loading core modules',
        '[ OK ] Mounting file system',
        '[ OK ] Initializing TUI renderer (ratatui)',
        '[ OK ] Loading configuration',
        '[ OK ] Checking API credentials',
        '[ OK ] Starting panel manager',
        '[ OK ] Activating context tracker',
        '',
        'MODULES LOADED:',
        '  ├─ Files Module      [ACTIVE]',
        '  ├─ Tree Module       [ACTIVE]',
        '  ├─ Git Module        [ACTIVE]',
        '  ├─ Console Module    [ACTIVE]',
        '  ├─ Todo Module       [ACTIVE]',
        '  ├─ Memory Module     [ACTIVE]',
        '  └─ Spine Module      [ACTIVE]',
        '',
        'SYSTEM STATUS: OPERATIONAL',
        'CONTEXT WINDOW: 200,000 tokens available',
        'AI PROVIDER: Ready',
        '',
        '> Starting user interface...',
        ''
    ];
    
    let currentLine = 0;
    
    function typeLine() {
        if (currentLine < bootLines.length) {
            bootText.textContent += bootLines[currentLine] + '\n';
            currentLine++;
            
            // Scroll to bottom
            bootSeq.scrollTop = bootSeq.scrollHeight;
            
            // Random delay for realistic boot sequence
            const delay = bootLines[currentLine - 1] === '' ? 50 : (Math.random() * 100 + 50);
            setTimeout(typeLine, delay);
        } else {
            // Boot complete
            setTimeout(() => {
                bootSeq.style.display = 'none';
                mainInterface.classList.remove('hidden');
                state.bootComplete = true;
                unlockAchievement('first_boot');
                loadPanel('welcome');
                
                // Focus input
                document.getElementById('command-input').focus();
            }, 500);
        }
    }
    
    typeLine();
}

// ===== Panel Management =====
function loadPanel(panelName) {
    if (!state.unlockedPanels.has(panelName)) {
        showOutput('⚠ PANEL LOCKED. Complete previous sections to unlock.', 'error');
        return;
    }
    
    state.currentPanel = panelName;
    const content = panelContent[panelName];
    
    // Update sidebar
    document.querySelectorAll('.panel-item').forEach(item => {
        item.classList.remove('active');
        if (item.dataset.panel === panelName) {
            item.classList.add('active');
        }
    });
    
    // Update content
    const display = document.getElementById('content-display');
    display.innerHTML = `
        <div class="section-title">${content.title}</div>
        ${content.content}
    `;
    
    // Scroll to top
    display.scrollTop = 0;
    
    // Update XP display
    updateXPDisplay();
    
    // Give XP for visiting new panels
    if (panelName !== 'welcome') {
        giveXP(25);
    }
}

function unlockPanel(panelName) {
    state.unlockedPanels.add(panelName);
    const panelItem = document.querySelector(`[data-panel="${panelName}"]`);
    if (panelItem) {
        panelItem.classList.remove('locked');
    }
}

// ===== Command System =====
const commands = {
    help: {
        desc: 'Show all available commands',
        exec: () => {
            unlockAchievement('help_seeker');
            return `
<div class="content-section">
<div class="section-title">» AVAILABLE COMMANDS</div>

<strong style="color: var(--crt-highlight)">Navigation:</strong>
  <code class="code-inline">next</code>, <code class="code-inline">n</code>          Go to next panel
  <code class="code-inline">prev</code>, <code class="code-inline">p</code>          Go to previous panel
  <code class="code-inline">welcome</code>               Jump to welcome screen
  <code class="code-inline">features</code>              View features
  <code class="code-inline">modules</code>               View modules
  <code class="code-inline">demo</code>                  Interactive demo
  <code class="code-inline">install</code>               Installation guide

<strong style="color: var(--crt-highlight)">Information:</strong>
  <code class="code-inline">help</code>                  Show this help
  <code class="code-inline">status</code>                Show current status
  <code class="code-inline">achievements</code>          List achievements
  <code class="code-inline">about</code>                 About Context Pilot

<strong style="color: var(--crt-highlight)">Utility:</strong>
  <code class="code-inline">clear</code>                 Clear screen
  <code class="code-inline">github</code>                Open GitHub repo
  <code class="code-inline">docs</code>                  View documentation

<strong style="color: var(--crt-highlight)">Easter Eggs:</strong>
  <code class="code-inline">matrix</code>                ????
  <code class="code-inline">hack</code>                  ????

<span style="color: var(--crt-text-dim)">Type any command and press Enter</span>
</div>`;
        }
    },
    
    next: {
        desc: 'Go to next panel',
        exec: () => {
            const panels = ['welcome', 'features', 'modules', 'demo', 'install'];
            const currentIndex = panels.indexOf(state.currentPanel);
            const nextIndex = (currentIndex + 1) % panels.length;
            const nextPanel = panels[nextIndex];
            
            if (!state.unlockedPanels.has(nextPanel)) {
                unlockPanel(nextPanel);
                giveXP(50);
            }
            
            loadPanel(nextPanel);
            return null; // Don't show output, just navigate
        }
    },
    
    n: { desc: 'Alias for next', exec: () => commands.next.exec() },
    
    prev: {
        desc: 'Go to previous panel',
        exec: () => {
            const panels = ['welcome', 'features', 'modules', 'demo', 'install'];
            const currentIndex = panels.indexOf(state.currentPanel);
            const prevIndex = (currentIndex - 1 + panels.length) % panels.length;
            loadPanel(panels[prevIndex]);
            return null;
        }
    },
    
    p: { desc: 'Alias for prev', exec: () => commands.prev.exec() },
    
    welcome: { desc: 'Jump to welcome', exec: () => { loadPanel('welcome'); return null; } },
    features: { desc: 'Jump to features', exec: () => { 
        if (!state.unlockedPanels.has('features')) {
            unlockPanel('features');
            giveXP(50);
        }
        loadPanel('features'); 
        return null; 
    }},
    modules: { desc: 'Jump to modules', exec: () => { 
        if (!state.unlockedPanels.has('modules')) {
            unlockPanel('modules');
            giveXP(50);
        }
        loadPanel('modules'); 
        return null; 
    }},
    demo: { desc: 'Jump to demo', exec: () => { 
        if (!state.unlockedPanels.has('demo')) {
            unlockPanel('demo');
            giveXP(75);
        }
        loadPanel('demo'); 
        return null; 
    }},
    install: { desc: 'Jump to install', exec: () => { 
        if (!state.unlockedPanels.has('install')) {
            unlockPanel('install');
            giveXP(100);
        }
        loadPanel('install'); 
        return null; 
    }},
    
    status: {
        desc: 'Show current status',
        exec: () => {
            return `
<div class="content-section">
<div class="section-title">» SYSTEM STATUS</div>
<strong>Current Panel:</strong> ${state.currentPanel.toUpperCase()}
<strong>Level:</strong> ${state.userLevel}
<strong>XP:</strong> ${state.userXP} / ${getXPForNextLevel()}
<strong>Achievements:</strong> ${state.achievements.size} / ${Object.keys(achievements).length}
<strong>Unlocked Panels:</strong> ${state.unlockedPanels.size} / 5

<strong style="color: var(--crt-highlight)">UNLOCKED PANELS:</strong>
${Array.from(state.unlockedPanels).map(p => `  ✓ ${p}`).join('\n')}
</div>`;
        }
    },
    
    achievements: {
        desc: 'List achievements',
        exec: () => {
            let output = '<div class="content-section"><div class="section-title">» ACHIEVEMENTS</div>\n';
            
            for (const [id, achievement] of Object.entries(achievements)) {
                const unlocked = state.achievements.has(id);
                const icon = unlocked ? '🏆' : '🔒';
                const status = unlocked ? '<span style="color: var(--crt-green)">UNLOCKED</span>' : '<span style="color: var(--crt-text-dim)">LOCKED</span>';
                output += `\n<div class="feature-box">
  ${icon} <strong>${achievement.name}</strong> - ${status}
  ${achievement.desc} ${unlocked ? `(+${achievement.xp} XP)` : ''}
</div>`;
            }
            
            output += '</div>';
            return output;
        }
    },
    
    about: {
        desc: 'About Context Pilot',
        exec: () => {
            return `
<div class="content-section">
<div class="section-title">» ABOUT CONTEXT PILOT</div>

<strong style="color: var(--crt-highlight)">Context Pilot v2.0.1</strong>
AI-Native Terminal IDE

<strong>Built with:</strong>
  • Rust (blazing fast, memory safe)
  • ratatui (terminal UI framework)
  • tokio (async runtime)

<strong>Created by:</strong>
  bigmoostache

<strong>License:</strong>
  GPL-3.0

<strong>Links:</strong>
  • GitHub: <a href="https://github.com/bigmoostache/context-pilot" style="color: var(--crt-cyan)">github.com/bigmoostache/context-pilot</a>
  • Issues: <a href="https://github.com/bigmoostache/context-pilot/issues" style="color: var(--crt-cyan)">Report bugs or request features</a>

<em style="color: var(--crt-text-dim)">Your AI coding assistant has amnesia. This one doesn't.</em>
</div>`;
        }
    },
    
    clear: {
        desc: 'Clear screen',
        exec: () => {
            loadPanel(state.currentPanel);
            return null;
        }
    },
    
    github: {
        desc: 'Open GitHub repo',
        exec: () => {
            unlockAchievement('readme');
            window.open('https://github.com/bigmoostache/context-pilot', '_blank');
            return '<span style="color: var(--crt-green)">✓</span> Opening GitHub repository in new tab...';
        }
    },
    
    docs: {
        desc: 'View documentation',
        exec: () => {
            unlockAchievement('readme');
            return `
<div class="content-section">
<div class="section-title">» DOCUMENTATION</div>

<strong>Full documentation available on GitHub:</strong>
<a href="https://github.com/bigmoostache/context-pilot" style="color: var(--crt-cyan)">github.com/bigmoostache/context-pilot</a>

<strong>Key Resources:</strong>
  • README.md - Overview and quick start
  • docs/ - Detailed guides
  • CONTRIBUTING.md - How to contribute
  • examples/ - Example configurations

<strong>In-App Help:</strong>
  Type <code class="code-inline">help</code> for command list
  Type <code class="code-inline">status</code> for current state
</div>`;
        }
    },
    
    matrix: {
        desc: '????',
        exec: () => {
            giveXP(25);
            return `
<div class="ascii-art" style="color: var(--crt-green); font-size: 10px;">
01001000 01100001 01100011 01101011 00100000 01110100 01101000 01100101
00100000 01110000 01101100 01100001 01101110 01100101 01110100 00100001

Wake up, Neo...
The Matrix has you...
Follow the white rabbit...

<span style="color: var(--crt-highlight)">But first, take control of your AI's context.</span>
</div>`;
        }
    },
    
    hack: {
        desc: '????',
        exec: () => {
            giveXP(50);
            return `
<div class="content-section">
<div style="color: var(--crt-green); font-family: var(--font-mono);">
> ACCESSING MAINFRAME...
> BYPASSING FIREWALL...
> DECRYPTING DATABASE...
> 
> [ OK ] ACCESS GRANTED
> 
> SYSTEM FILES:
>   /etc/shadow ......................... [ENCRYPTED]
>   /root/.secrets ...................... [ENCRYPTED]
>   /home/user/context-pilot ............ [READABLE]
> 
> Reality: You're not hacking anything.
> This is just a demo website.
> 
> But Context Pilot *does* give you full control
> over something much more valuable:
> 
> <span style="color: var(--crt-highlight);">YOUR AI'S CONTEXT WINDOW.</span>
> 
> Now that's real power. 🚀
</div>
</div>`;
        }
    }
};

function executeCommand(input) {
    const trimmed = input.trim().toLowerCase();
    
    if (!trimmed) return;
    
    // Add to history
    state.commandHistory.push(trimmed);
    state.historyIndex = state.commandHistory.length;
    
    // Check if command exists
    if (commands[trimmed]) {
        const result = commands[trimmed].exec();
        if (result) {
            showOutput(result);
        }
        giveXP(10);
    } else {
        showOutput(`<span style="color: var(--crt-red);">⚠ Unknown command: "${input}"</span>
Type <code class="code-inline">help</code> for available commands.`, 'error');
        showSoundEffect('*ERROR*', window.innerWidth / 2, 100, 'var(--crt-red)');
    }
}

function showOutput(html, type = 'success') {
    const display = document.getElementById('content-display');
    const output = document.createElement('div');
    output.className = 'command-output';
    output.innerHTML = html;
    display.innerHTML = '';
    display.appendChild(output);
    display.scrollTop = 0;
}

// ===== XP and Leveling =====
function giveXP(amount) {
    state.userXP += amount;
    
    // Create particle at random position near center
    const x = window.innerWidth / 2 + (Math.random() - 0.5) * 100;
    const y = window.innerHeight / 2 + (Math.random() - 0.5) * 100;
    createXPParticle(amount, x, y);
    
    // Update combo
    updateCombo();
    
    // Check for level up
    const xpNeeded = getXPForNextLevel();
    if (state.userXP >= xpNeeded) {
        state.userLevel++;
        state.userXP -= xpNeeded;
        unlockAchievement('level_up');
        showLevelUpScreen();
    }
    
    // Update high score
    const totalScore = state.userLevel * 1000 + state.userXP;
    if (totalScore > state.highScore) {
        state.highScore = totalScore;
        localStorage.setItem('contextPilotHighScore', state.highScore);
        updateHighScore();
    }
    
    updateXPDisplay();
}

function getXPForNextLevel() {
    return 250 * state.userLevel;
}

function updateXPDisplay() {
    document.getElementById('user-xp').textContent = `XP: ${state.userXP}`;
    document.getElementById('user-level').textContent = `LEVEL: ${state.userLevel}`;
    
    const xpProgress = document.getElementById('xp-progress');
    const progressFill = document.getElementById('progress-fill');
    
    if (xpProgress && progressFill) {
        const needed = getXPForNextLevel();
        const percent = (state.userXP / needed) * 100;
        xpProgress.textContent = `${state.userXP} / ${needed} XP`;
        progressFill.style.width = `${percent}%`;
    }
}

function showLevelUpNotification() {
    const popup = document.getElementById('achievement-popup');
    const name = document.getElementById('achievement-name');
    const xpEl = document.getElementById('achievement-xp');
    
    name.textContent = `LEVEL ${state.userLevel} REACHED!`;
    xpEl.textContent = '0';
    
    popup.classList.remove('hidden');
    
    setTimeout(() => {
        popup.classList.add('hidden');
    }, 3000);
}

// ===== Achievement System =====
function unlockAchievement(achievementId) {
    if (state.achievements.has(achievementId)) return;
    
    state.achievements.add(achievementId);
    const achievement = achievements[achievementId];
    
    // Give XP
    if (achievement.xp > 0) {
        giveXP(achievement.xp);
    }
    
    // Show notification
    const popup = document.getElementById('achievement-popup');
    const name = document.getElementById('achievement-name');
    const xpEl = document.getElementById('achievement-xp');
    
    name.textContent = achievement.name;
    xpEl.textContent = achievement.xp;
    
    popup.classList.remove('hidden');
    
    setTimeout(() => {
        popup.classList.add('hidden');
    }, 3000);
    
    // Update count
    document.getElementById('achievement-count').textContent = state.achievements.size;
    
    // Check for explorer achievement
    if (state.unlockedPanels.size === 5 && !state.achievements.has('explorer')) {
        setTimeout(() => unlockAchievement('explorer'), 3500);
    }
    
    // Check for completionist
    if (state.achievements.size === Object.keys(achievements).length - 1) {
        setTimeout(() => unlockAchievement('completionist'), 3500);
    }
}

// ===== Event Handlers =====
document.addEventListener('DOMContentLoaded', () => {
    // Start boot sequence
    runBootSequence();
    
    // Command input handler
    const input = document.getElementById('command-input');
    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            const command = input.value;
            executeCommand(command);
            input.value = '';
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            if (state.historyIndex > 0) {
                state.historyIndex--;
                input.value = state.commandHistory[state.historyIndex] || '';
            }
        } else if (e.key === 'ArrowDown') {
            e.preventDefault();
            if (state.historyIndex < state.commandHistory.length) {
                state.historyIndex++;
                input.value = state.commandHistory[state.historyIndex] || '';
            }
        }
    });
    
    // Sidebar panel clicks
    document.querySelectorAll('.panel-item').forEach(item => {
        item.addEventListener('click', () => {
            const panel = item.dataset.panel;
            if (!item.classList.contains('locked')) {
                loadPanel(panel);
            } else {
                showOutput('<span style="color: var(--crt-amber);">⚠ PANEL LOCKED</span>\nProgress through the demo to unlock more panels.\nTry typing <code class="code-inline">next</code> to continue.', 'error');
            }
        });
    });
    
    // Hint click handlers
    document.addEventListener('click', (e) => {
        if (e.target.classList.contains('cmd-hint')) {
            const cmd = e.target.textContent;
            executeCommand(cmd);
        }
    });
    
    // Apply saved theme
    applyTheme(state.theme);
    
    // Show high score
    updateHighScore();
    
    // Konami code listener
    document.addEventListener('keydown', (e) => {
        state.konamiCode.push(e.key);
        if (state.konamiCode.length > 10) {
            state.konamiCode.shift();
        }
        
        if (JSON.stringify(state.konamiCode) === JSON.stringify(state.konamiSequence)) {
            activateKonamiCode();
            state.konamiCode = [];
        }
    });
});

// ===== ENHANCED GAMIFICATION FUNCTIONS =====

// XP Particles
function createXPParticle(amount, x, y) {
    const particle = document.createElement('div');
    particle.className = 'xp-particle';
    particle.textContent = `+${amount} XP`;
    particle.style.left = x + 'px';
    particle.style.top = y + 'px';
    document.body.appendChild(particle);
    
    // Play sound effect
    showSoundEffect('*DING*', x, y - 30, 'var(--crt-yellow)');
    
    setTimeout(() => particle.remove(), 2000);
}

// Sound Effect Text
function showSoundEffect(text, x, y, color) {
    const effect = document.createElement('div');
    effect.className = 'sound-effect';
    effect.textContent = text;
    effect.style.left = x + 'px';
    effect.style.top = y + 'px';
    effect.style.color = color;
    document.body.appendChild(effect);
    
    setTimeout(() => effect.remove(), 600);
}

// Enhanced XP giving with particles
function giveXP(amount) {
    state.userXP += amount;
    
    // Create particle at random position near center
    const x = window.innerWidth / 2 + (Math.random() - 0.5) * 100;
    const y = window.innerHeight / 2 + (Math.random() - 0.5) * 100;
    createXPParticle(amount, x, y);
    
    // Update combo
    updateCombo();
    
    // Check for level up
    const xpNeeded = getXPForNextLevel();
    if (state.userXP >= xpNeeded) {
        state.userLevel++;
        state.userXP -= xpNeeded;
        unlockAchievement('level_up');
        showLevelUpScreen();
    }
    
    // Update high score
    const totalScore = state.userLevel * 1000 + state.userXP;
    if (totalScore > state.highScore) {
        state.highScore = totalScore;
        localStorage.setItem('contextPilotHighScore', state.highScore);
        updateHighScore();
    }
    
    updateXPDisplay();
}

// Combo System
function updateCombo() {
    const now = Date.now();
    const timeSinceLastCommand = now - state.lastCommandTime;
    
    // Reset combo if too slow (more than 3 seconds)
    if (timeSinceLastCommand > 3000) {
        state.combo = 1;
    } else {
        state.combo++;
        
        // Show combo counter
        if (state.combo > 1) {
            showComboCounter();
        }
        
        // Bonus XP for combos
        if (state.combo >= 5) {
            const bonus = Math.floor(state.combo / 5) * 25;
            state.userXP += bonus;
            showSoundEffect('*COMBO!*', window.innerWidth - 100, 150, 'var(--crt-orange)');
        }
    }
    
    state.lastCommandTime = now;
    
    // Reset combo after delay
    clearTimeout(state.comboTimer);
    state.comboTimer = setTimeout(() => {
        state.combo = 0;
        hideComboCounter();
    }, 3000);
}

function showComboCounter() {
    let counter = document.querySelector('.combo-counter');
    if (!counter) {
        counter = document.createElement('div');
        counter.className = 'combo-counter';
        counter.innerHTML = `
            <span class="combo-number"></span>
            <span class="combo-label">COMBO!</span>
        `;
        document.body.appendChild(counter);
    }
    
    counter.querySelector('.combo-number').textContent = `x${state.combo}`;
    counter.style.display = 'block';
}

function hideComboCounter() {
    const counter = document.querySelector('.combo-counter');
    if (counter) {
        counter.style.display = 'none';
    }
}

// Level Up Screen
function showLevelUpScreen() {
    const overlay = document.createElement('div');
    overlay.className = 'level-up-overlay';
    overlay.innerHTML = `
        <div class="level-up-content">
            <div class="level-up-title">*** LEVEL UP! ***</div>
            <span class="level-up-number">${state.userLevel}</span>
            <div class="level-up-subtitle">Press any key to continue</div>
        </div>
    `;
    document.body.appendChild(overlay);
    
    // Play sound effects
    setTimeout(() => showSoundEffect('*POWER UP!*', window.innerWidth / 2, window.innerHeight / 3, 'var(--crt-yellow)'), 300);
    
    // Remove on any key
    const removeOverlay = () => {
        overlay.remove();
        document.removeEventListener('keydown', removeOverlay);
        document.removeEventListener('click', removeOverlay);
    };
    
    document.addEventListener('keydown', removeOverlay);
    document.addEventListener('click', removeOverlay);
}

// Theme Switcher
function applyTheme(theme) {
    document.body.className = `theme-${theme}`;
    state.theme = theme;
    localStorage.setItem('contextPilotTheme', theme);
    
    // Update active button
    document.querySelectorAll('.theme-btn').forEach(btn => {
        btn.classList.remove('active');
    });
    const activeBtn = document.querySelector(`.theme-btn.${theme}`);
    if (activeBtn) {
        activeBtn.classList.add('active');
    }
}

function createThemeSwitcher() {
    const switcher = document.createElement('div');
    switcher.className = 'theme-switcher';
    switcher.innerHTML = `
        <button class="theme-btn green active" title="Green Phosphor" data-theme="green"></button>
        <button class="theme-btn amber" title="Amber Phosphor" data-theme="amber"></button>
        <button class="theme-btn cyan" title="Cyan Phosphor" data-theme="cyan"></button>
    `;
    document.body.appendChild(switcher);
    
    // Add click handlers
    switcher.querySelectorAll('.theme-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            const theme = btn.dataset.theme;
            applyTheme(theme);
            showSoundEffect('*BEEP*', 70, window.innerHeight - 120, `var(--crt-${theme})`);
        });
    });
}

// High Score Display
function updateHighScore() {
    let scoreDisplay = document.querySelector('.high-score');
    if (!scoreDisplay) {
        scoreDisplay = document.createElement('div');
        scoreDisplay.className = 'high-score';
        document.body.appendChild(scoreDisplay);
    }
    scoreDisplay.innerHTML = `
        <div>HIGH SCORE</div>
        <div style="font-size: 18px;">${state.highScore.toLocaleString()}</div>
    `;
}

// Konami Code Easter Egg
function activateKonamiCode() {
    unlockAchievement('completionist');
    
    // Unlock all panels
    ['welcome', 'features', 'modules', 'demo', 'install'].forEach(panel => {
        unlockPanel(panel);
    });
    
    // Give massive XP
    giveXP(1000);
    
    // Show special message
    showOutput(`
<div class="ascii-art" style="color: var(--crt-purple); font-size: 14px;">
╔════════════════════════════════════════════════════════════╗
║                                                            ║
║   ██╗  ██╗ ██████╗ ███╗   ██╗ █████╗ ███╗   ███╗██╗      ║
║   ██║ ██╔╝██╔═══██╗████╗  ██║██╔══██╗████╗ ████║██║      ║
║   █████╔╝ ██║   ██║██╔██╗ ██║███████║██╔████╔██║██║      ║
║   ██╔═██╗ ██║   ██║██║╚██╗██║██╔══██║██║╚██╔╝██║██║      ║
║   ██║  ██╗╚██████╔╝██║ ╚████║██║  ██║██║ ╚═╝ ██║██║      ║
║   ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚═╝     ╚═╝╚═╝      ║
║                                                            ║
║              CODE ACTIVATED: GOD MODE                      ║
║                                                            ║
║   ✓ All panels unlocked                                   ║
║   ✓ +1000 XP bonus                                        ║
║   ✓ Achievement: "Completionist" unlocked                 ║
║                                                            ║
║   <span style="color: var(--crt-highlight);">You are a true retro gamer.</span>                         ║
║                                                            ║
╚════════════════════════════════════════════════════════════╝
</div>
    `);
    
    showSoundEffect('*1UP!*', window.innerWidth / 2, window.innerHeight / 2, 'var(--crt-purple)');
}

// Initialize gamification features after boot
setTimeout(() => {
    if (state.bootComplete) {
        createThemeSwitcher();
        updateHighScore();
    }
}, 5000);
