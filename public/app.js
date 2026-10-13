const PALETTE = ['#dceaf4', '#f8f8ba', '#f8e4e6', '#dfe6cd']; 
let FEED = [];
const FEED_PAGE_SIZE = 20;
let feedPage = 0;
let hasMoreFeed = true;
let feedRequest = null;

function getOrCreateSessionSeed() {
    let seed = sessionStorage.getItem('ffkwijt_seed');
    if (!seed) {
        seed = crypto.randomUUID();
        sessionStorage.setItem('ffkwijt_seed', seed);
    }
    return seed;
}

let feedSeed = getOrCreateSessionSeed();

function hashDJB2(str) {
    let hash = 5381;
    for (let i = 0; i < str.length; i++) {
        hash = ((hash << 5) + hash) + str.charCodeAt(i);
        hash = hash & hash;
    }
    return Math.abs(hash);
}

function getNoteStyle(id) {
    const h = hashDJB2(id || 'default');
    const normAngle = (h % 1000) / 1000;
    const angle = ((normAngle * 2.9) - 1.45).toFixed(2);
    const tapeShift = (((h >> 3) % 9) - 4);
    const tapeTilt = (((((h >> 5) % 100) / 100) * 2.4) - 1.2).toFixed(2);
    return {
        angle: parseFloat(angle),
        tapeTransform: `translateX(calc(-50% + ${tapeShift}px)) rotate(${tapeTilt}deg)`
    };
}

let currentIndex = 0;
let dragY = 0;
let isAnimating = false;
let isWriting = false;
let isDisconnected = false;
let isRefreshing = false;

const viewport = document.getElementById('viewport');
const prevCard = document.getElementById('prev-card');
const currentCard = document.getElementById('current-card');
const nextCard = document.getElementById('next-card');

const prevText = document.getElementById('prev-text');
const currentText = document.getElementById('current-text');
const nextText = document.getElementById('next-text');

const writeToggleBtn = document.getElementById('write-toggle');
const composeModal = document.getElementById('compose-modal');
const composeInput = document.getElementById('compose-input');
const composeWebsite = document.getElementById('compose-website'); // Honeypot
const composeCount = document.getElementById('compose-count');
const composeCancel = document.getElementById('compose-cancel');
const composeSubmit = document.getElementById('compose-submit');
const navHint = document.getElementById('nav-hint');
const feedLoader = document.getElementById('feed-loader');

const crisisModal = document.getElementById('crisis-modal');
const crisisCloseBtn = document.getElementById('crisis-close');

const localResonated = new Set(JSON.parse(localStorage.getItem('ffkwijt_resonated') || '[]'));

function configureCard(cardEl, textEl, item, paletteIdx) {
    if (!item) {
        textEl.textContent = 'Even stil... Geen berichten gevonden.';
        return;
    }
    const styleMeta = getNoteStyle(item.id);

    textEl.textContent = item.text ?? item.t ?? '';
    cardEl.style.backgroundColor = PALETTE[paletteIdx % PALETTE.length];
    cardEl.dataset.baseAngle = styleMeta.angle;

    const tape = cardEl.querySelector('.note-tape');
    if (tape) tape.style.transform = styleMeta.tapeTransform;

    const holdBtn = cardEl.querySelector('.hold-trigger');
    if (holdBtn) {
        if (localResonated.has(item.id)) {
            holdBtn.classList.add('is-held');
        } else {
            holdBtn.classList.remove('is-held');
        }

        holdBtn.onclick = async (e) => {
            e.stopPropagation();
            if (localResonated.has(item.id)) return;

            localResonated.add(item.id);
            localStorage.setItem('ffkwijt_resonated', JSON.stringify(Array.from(localResonated)));
            holdBtn.classList.add('is-held');

            try {
                await fetch(`/api/resonate/${encodeURIComponent(item.id)}`, {
                    method: 'POST'
                });
            } catch (err) {
                console.error('Failed to resonate:', err);
            }
        };
    }
}

function initCards() {
    if (FEED.length === 0) {
        currentText.textContent = 'Even stil... Geen berichten gevonden.';
        prevCard.style.display = 'none';
        nextCard.style.display = 'none';
        applyTransforms(0);
        return;
    }

    const hasPrev = currentIndex > 0;
    if (hasPrev) {
        prevCard.style.display = 'flex';
        configureCard(prevCard, prevText, FEED[currentIndex - 1], currentIndex - 1);
    } else {
        prevCard.style.display = 'none';
    }

    configureCard(currentCard, currentText, FEED[currentIndex], currentIndex);

    const nextIndex = currentIndex + 1 < FEED.length ? currentIndex + 1 :
        (!hasMoreFeed && FEED.length > 1 ? 0 : -1);
    if (nextIndex !== -1) {
        nextCard.style.display = 'flex';
        configureCard(nextCard, nextText, FEED[nextIndex], nextIndex);
    } else {
        nextCard.style.display = 'none';
    }

    applyTransforms(0);
}

function applyTransforms(deltaY) {
    if (isWriting) return;

    let effectiveDeltaY = deltaY;
    if ((currentIndex === 0 && deltaY > 0) ||
        (currentIndex >= FEED.length - 1 && deltaY < 0 && (hasMoreFeed || FEED.length <= 1))) {
        effectiveDeltaY = deltaY * 0.25;
    }

    const windowH = cachedWindowH;
    const progress = Math.min(Math.abs(effectiveDeltaY) / (windowH * 0.75), 1);

    if (!isRefreshing) {
        feedLoader.hidden = !(currentIndex === 0 && deltaY > 8);
        feedLoader.style.opacity = `${Math.min(Math.max(deltaY, 0) / 70, 1)}`;
    }

    // Alleen schalen tijdens actieve drag; in ruststand exact 'none' zodat tekst 100% scherp blijft
    if (effectiveDeltaY === 0) {
        viewport.style.transform = 'none';
    } else {
        const cameraScale = (1 - (progress * 0.08)).toFixed(4);
        viewport.style.transform = `scale(${cameraScale})`;
    }

    const roundedY = Math.round(effectiveDeltaY);
    const currAngle = parseFloat(currentCard.dataset.baseAngle || 0);
    currentCard.style.transform = `translate3d(0, ${roundedY}px, 0) rotate(${currAngle}deg)`;
    currentCard.style.opacity = `${(1 - (progress * 0.35)).toFixed(3)}`;

    if (FEED.length > 1) {
        const nextAngle = parseFloat(nextCard.dataset.baseAngle || 0);
        const nextY = Math.round((windowH * 0.86) + effectiveDeltaY);
        nextCard.style.transform = `translate3d(0, ${Math.max(nextY, 0)}px, 0) rotate(${nextAngle}deg)`;
        nextCard.style.opacity = effectiveDeltaY < 0 ? `${(0.2 + (progress * 0.8)).toFixed(3)}` : '0';
    }

    if (currentIndex > 0) {
        const prevAngle = parseFloat(prevCard.dataset.baseAngle || 0);
        const prevY = Math.round((-windowH * 0.86) + effectiveDeltaY);
        prevCard.style.transform = `translate3d(0, ${Math.min(prevY, 0)}px, 0) rotate(${prevAngle}deg)`;
        prevCard.style.opacity = effectiveDeltaY > 0 ? `${(0.2 + (progress * 0.8)).toFixed(3)}` : '0';
    }
}

async function completeTransition(direction) {
    if (isDisconnected || isRefreshing || isWriting || isAnimating) return;

    if (direction === 1 && currentIndex + 1 >= FEED.length && hasMoreFeed) {
        await loadFeedWithSpinner({ append: true });
        if (isDisconnected || isWriting || isAnimating) return;
    }

    const targetIndex = direction === 1 && currentIndex + 1 >= FEED.length && !hasMoreFeed ?
        0 : currentIndex + direction;
    if (FEED.length <= 1 || targetIndex < 0 || targetIndex >= FEED.length) {
        cancelTransition();
        return;
    }

    const targetId = FEED[targetIndex].id;
    initCards();
    applyTransforms(dragY);

    isAnimating = true;
    const windowH = cachedWindowH;
    const duration = 380;
    const ease = 'cubic-bezier(0.22, 1, 0.36, 1)';

    currentCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    nextCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    prevCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    viewport.style.transition = `transform ${duration}ms ${ease}`;

    const targetYOut = Math.round(windowH * 0.95);

    if (direction === 1) {
        const currAngle = parseFloat(currentCard.dataset.baseAngle || 0);
        currentCard.style.transform = `translate3d(0, ${-targetYOut}px, 0) rotate(${currAngle}deg)`;
        currentCard.style.opacity = '0';

        const nextAngle = parseFloat(nextCard.dataset.baseAngle || 0);
        nextCard.style.transform = `translate3d(0, 0, 0) rotate(${nextAngle}deg)`;
        nextCard.style.opacity = '1';
    } else {
        const currAngle = parseFloat(currentCard.dataset.baseAngle || 0);
        currentCard.style.transform = `translate3d(0, ${targetYOut}px, 0) rotate(${currAngle}deg)`;
        currentCard.style.opacity = '0';

        const prevAngle = parseFloat(prevCard.dataset.baseAngle || 0);
        prevCard.style.transform = `translate3d(0, 0, 0) rotate(${prevAngle}deg)`;
        prevCard.style.opacity = '1';
    }

    viewport.style.transform = 'none';

    setTimeout(() => {
        currentCard.style.transition = '';
        nextCard.style.transition = '';
        prevCard.style.transition = '';
        viewport.style.transition = '';

        currentIndex = FEED.findIndex(item => item.id === targetId);
        initCards();
        dragY = 0;
        isAnimating = false;
        if (hasMoreFeed && FEED.length - currentIndex <= 5) {
            fetchFeed({ append: true });
        }
    }, duration);
}

function cancelTransition() {
    if (isWriting || isAnimating) return;
    isAnimating = true;
    const duration = 260;
    const ease = 'cubic-bezier(0.25, 1, 0.5, 1)';

    currentCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    nextCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    prevCard.style.transition = `transform ${duration}ms ${ease}, opacity ${duration}ms ease`;
    viewport.style.transition = `transform ${duration}ms ${ease}`;

    applyTransforms(0);

    setTimeout(() => {
        currentCard.style.transition = '';
        nextCard.style.transition = '';
        prevCard.style.transition = '';
        viewport.style.transition = '';
        dragY = 0;
        isAnimating = false;
    }, duration);
}

function openCompose() {
    if (isDisconnected || isRefreshing || isAnimating) return;
    isWriting = true;
    dragY = 0;

    currentCard.style.transition = 'transform 0.3s ease, opacity 0.3s ease';
    currentCard.style.transform = 'scale(0.96)';
    currentCard.style.opacity = '0.4';
    prevCard.style.opacity = '0';
    nextCard.style.opacity = '0';

    composeModal.classList.add('is-active');
    navHint.style.opacity = '0';
    writeToggleBtn.style.opacity = '0';
    writeToggleBtn.style.pointerEvents = 'none';

    composeInput.value = '';
    if (composeWebsite) composeWebsite.value = '';
    composeCount.textContent = '0 / 280';

    setTimeout(() => { composeInput.focus(); }, 100);
}

function closeCompose() {
    composeInput.blur();
    composeModal.classList.remove('is-active');

    setTimeout(() => {
        isWriting = false;
        dragY = 0;
        isAnimating = false;

        writeToggleBtn.style.opacity = '1';
        writeToggleBtn.style.pointerEvents = 'auto';
        navHint.style.opacity = '1';

        currentCard.style.transition = 'transform 0.3s ease, opacity 0.3s ease';
        currentCard.style.opacity = '1';

        initCards();

        setTimeout(() => {
            currentCard.style.transition = '';
            prevCard.style.transition = '';
            nextCard.style.transition = '';
        }, 320);
    }, 200);
}

// 1. Initial backlog ophalen
async function fetchFeed({ append = false, resetSeed = false } = {}) {
    if (append) {
        if (feedRequest) return feedRequest.promise;
        if (!hasMoreFeed) return true;
    } else {
        feedRequest?.controller.abort();
    }

    const seed = resetSeed ? (() => {
        const s = crypto.randomUUID();
        sessionStorage.setItem('ffkwijt_seed', s);
        return s;
    })() : feedSeed;

    const page = append ? feedPage + 1 : 1;
    const params = new URLSearchParams({ page: `${page}`, limit: `${FEED_PAGE_SIZE}`, seed });
    const request = { controller: new AbortController(), promise: null, append, liveNotes: [] };
    feedRequest = request;

    request.promise = (async () => {
        try {
            const res = await fetch(`/api/feed?${params}`, {
                cache: 'no-store',
                signal: request.controller.signal
            });
            if (!res.ok) throw new Error(`HTTP ${res.status}`);
            const notes = await res.json();
            
            if (!Array.isArray(notes) || notes.some(note => {
                const textVal = note?.text ?? note?.t;
                return !note || typeof note.id !== 'string' || typeof textVal !== 'string';
            })) {
                throw new Error('Ongeldige feed');
            }
            if (feedRequest !== request) return false;

            const merged = append ? [...FEED, ...notes] : [...notes, ...request.liveNotes];
            const seen = new Set();
            FEED = merged.filter(note => {
                if (seen.has(note.id)) return false;
                seen.add(note.id);
                return true;
            });
            feedSeed = seed;
            feedPage = page;
            hasMoreFeed = notes.length === FEED_PAGE_SIZE;
            if (!append) currentIndex = 0;
            if (!isAnimating && !isWriting && !isTrackingTouch && dragY === 0) initCards();
            return true;
        } catch (err) {
            if (err.name !== 'AbortError') console.error('Feed laden mislukt:', err);
            return false;
        } finally {
            if (feedRequest === request) feedRequest = null;
        }
    })();
    return request.promise;
}

async function loadFeedWithSpinner(options) {
    if (isDisconnected || isWriting || isAnimating || isRefreshing) return;
    isRefreshing = true;
    feedLoader.hidden = false;
    feedLoader.style.opacity = '1';
    feedLoader.classList.add('is-loading');
    viewport.setAttribute('aria-busy', 'true');
    applyTransforms(0);

    try {
        return await fetchFeed(options);
    } finally {
        isRefreshing = false;
        dragY = 0;
        feedLoader.classList.remove('is-loading');
        feedLoader.hidden = true;
        viewport.setAttribute('aria-busy', 'false');
        initCards();
    }
}

function refreshFeed() {
    return loadFeedWithSpinner({ resetSeed: true });
}

function finishGesture(threshold) {
    if (dragY > threshold && currentIndex === 0) {
        refreshFeed();
    } else if (dragY < -threshold) {
        completeTransition(1);
    } else if (dragY > threshold) {
        completeTransition(-1);
    } else {
        cancelTransition();
    }
}

// 2. Passieve SSE stream luisteraar
function setupSSE() {
    const sse = new EventSource('/api/feed/stream');

    sse.addEventListener('new_note', (e) => {
        try {
            const note = JSON.parse(e.data);

            if (feedRequest && !feedRequest.append) feedRequest.liveNotes.push(note);
            if (FEED.some(item => item.id === note.id)) return;

            if (FEED.length === 0) {
                FEED.push(note);
                currentIndex = 0;
                initCards();
                return;
            }

            const insertIndex = Math.floor(Math.random() * (FEED.length + 1));
            FEED.splice(insertIndex, 0, note);

            if (insertIndex <= currentIndex) {
                currentIndex += 1;
            }

            if (!isAnimating && !isWriting && !isRefreshing && !isTrackingTouch && dragY === 0) {
                initCards();
            }
        } catch (err) {
            console.error('Fout bij verwerken live note:', err);
        }
    });

    sse.onerror = (err) => {
        console.warn('SSE verbinding verbroken, browser reconnect automatisch...', err);
    };
}

// 3. Posten
async function handlePublish() {
    const text = composeInput.value.trim();
    if (!text) {
        closeCompose();
        return;
    }

    const honeypotVal = composeWebsite ? composeWebsite.value : '';
    composeSubmit.disabled = true;

    try {
        const res = await fetch('/api/vent', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                text: text,
                website: honeypotVal
            })
        });

        if (res.status === 422) {
            const err = await res.text();
            if (err.includes('CRISIS_DETECTED')) {
                closeCompose();
                crisisModal.style.display = 'flex';
            } else {
                alert(err);
            }
            return;
        }

        if (res.ok) {
            closeCompose();
        } else {
            const err = await res.text();
            alert(`Fout: ${err}`);
        }
    } catch (err) {
        alert('Er kon geen verbinding worden gemaakt met de server.');
    } finally {
        composeSubmit.disabled = false;
    }
}

crisisCloseBtn.addEventListener('click', () => {
    crisisModal.style.display = 'none';
});

writeToggleBtn.addEventListener('click', openCompose);
composeCancel.addEventListener('click', closeCompose);
composeSubmit.addEventListener('click', handlePublish);

composeInput.addEventListener('input', () => {
    composeCount.textContent = `${composeInput.value.length} / 280`;
});

// Touch & Gestures
let startY = 0;
let isTrackingTouch = false;

viewport.addEventListener('touchstart', (e) => {
    if (isDisconnected || isRefreshing || isAnimating || isWriting) return;
    if (e.target.closest('button')) return;

    isTrackingTouch = true;
    startY = e.touches[0].clientY;
    dragY = 0;
}, { passive: true });

viewport.addEventListener('touchmove', (e) => {
    if (!isTrackingTouch || isRefreshing || isAnimating || isWriting) return;
    dragY = e.touches[0].clientY - startY;
    applyTransforms(dragY);
}, { passive: true });

viewport.addEventListener('touchend', () => {
    if (!isTrackingTouch) return;
    isTrackingTouch = false;
    if (isDisconnected || isRefreshing || isAnimating || isWriting) return;

    finishGesture(70);
});

viewport.addEventListener('touchcancel', () => {
    if (!isTrackingTouch) return;
    isTrackingTouch = false;
    cancelTransition();
});

// Trackpad & Scrollwiel
let wheelTimeout;
window.addEventListener('wheel', (e) => {
    if (isDisconnected || isRefreshing || isAnimating || isWriting) return;

    const dampedDelta = Math.max(Math.min(e.deltaY * 0.7, 35), -35);
    dragY -= dampedDelta;
    applyTransforms(dragY);

    clearTimeout(wheelTimeout);
    wheelTimeout = setTimeout(() => {
        finishGesture(60);
    }, 75);
}, { passive: true });

// Toetsenbordnavigatie
window.addEventListener('keydown', (e) => {
    if (isDisconnected || isRefreshing) return;
    if (isWriting) {
        if (e.key === 'Escape') closeCompose();
        return;
    }
    if (isAnimating) return;

    if (e.code === 'ArrowDown' || e.code === 'Space') {
        e.preventDefault();
        completeTransition(1);
    } else if (e.code === 'ArrowUp') {
        e.preventDefault();
        completeTransition(-1);
    }
});

const leaveBtn = document.getElementById('leave-btn');
const reconnectModal = document.getElementById('reconnect-modal');
const reconnectBtn = document.getElementById('reconnect-btn');
const originalTitle = document.title;
const outOfFocusEnabled = new URLSearchParams(window.location.search).get('of') !== 'disabled';
const privacyContent = document.querySelectorAll('.top-bar, .ambient-board, #viewport, #crisis-modal, #nav-hint');

function escapeToGoogle() {
    if (typeof composeInput !== 'undefined' && composeInput) {
        composeInput.value = '';
    }

    sessionStorage.setItem('ffkwijt_left', '1');
    triggerDisconnectState();
    window.location.replace('https://www.google.nl');
}

leaveBtn.addEventListener('click', escapeToGoogle);

function triggerDisconnectState() {
    if (isDisconnected) return;
    isDisconnected = true;
    document.title = 'Error 500';
    document.body.classList.add('is-disconnected');
    privacyContent.forEach(element => { element.inert = true; });
    clearTimeout(wheelTimeout);
    isTrackingTouch = false;
    dragY = 0;

    if (isWriting) {
        composeInput.value = '';
        closeCompose();
    }

    reconnectModal.style.display = 'flex';
}

function reconnect() {
    if (document.hidden || !document.hasFocus()) return;
    sessionStorage.removeItem('ffkwijt_left');
    isDisconnected = false;
    document.title = originalTitle;
    document.body.classList.remove('is-disconnected');
    privacyContent.forEach(element => { element.inert = false; });
    reconnectModal.style.display = 'none';
    writeToggleBtn.focus();

    if (!FEED || FEED.length === 0) {
        if (!feedRequest) fetchFeed();
    }
}

reconnectBtn.addEventListener('click', reconnect);
if (outOfFocusEnabled) {
    window.addEventListener('blur', triggerDisconnectState);
    document.addEventListener('visibilitychange', () => {
        if (document.hidden) triggerDisconnectState();
    });
    window.addEventListener('focus', () => {
        if (isDisconnected) reconnectBtn.focus();
    });
}

function handleHistoryReturn() {
    if (outOfFocusEnabled) triggerDisconnectState();
    if (sessionStorage.getItem('ffkwijt_left') === '1') {
        window.location.replace('https://www.google.nl');
        return true;
    }
    return false;
}

window.addEventListener('pageshow', (event) => {
    if (event.persisted) handleHistoryReturn();
});



// Opstarten
let cachedWindowH = window.visualViewport?.height || window.clientHeight || 800;

requestAnimationFrame(() => {
    cachedWindowH = window.innerHeight;
});

window.addEventListener('resize', () => {
    cachedWindowH = window.innerHeight;
}, { passive: true });

const isHistoryReturn = performance.getEntriesByType('navigation')[0]?.type === 'back_forward';
if (!isHistoryReturn) sessionStorage.removeItem('ffkwijt_left');
if (!isHistoryReturn || !handleHistoryReturn()) {
    if (outOfFocusEnabled && document.hidden) triggerDisconnectState();
    fetchFeed().then(() => {
        if ('requestIdleCallback' in window) {
            requestIdleCallback(() => setupSSE(), { timeout: 2000 });
        } else {
            setTimeout(setupSSE, 1000);
        }
    });
}

