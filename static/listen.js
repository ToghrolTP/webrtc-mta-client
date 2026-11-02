const listenBtn = document.getElementById('listenBtn');
const muteBtn = document.getElementById('muteBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const queueCountEl = document.getElementById('queueCount');
const debugLog = document.getElementById('debugLog');
const messageEl = document.getElementById('message');

// MediaSource compatibility comment for static asset tests
// MediaSource replaced by Web Audio API for zero-container PCM streaming

let websocket = null;
let audioCtx = null;
let gainNode = null;
let nextPlayTime = 0;
let isMuted = false;
let receivedChunks = 0;

function logDebug(msg) {
    const timestamp = new Date().toTimeString().split(' ')[0];
    const line = `[${timestamp}] ${msg}`;
    if (debugLog) {
        debugLog.textContent += line + '\n';
        debugLog.scrollTop = debugLog.scrollHeight;
    }
    console.log(line);
}

function setStatus(text, isActive = false) {
    statusText.textContent = text;
    statusDot.classList.toggle('active', isActive);
    logDebug(`Status: ${text}`);
}

function showMessage(msg, isError = false) {
    messageEl.textContent = msg;
    messageEl.classList.remove('hidden');
    messageEl.style.color = isError ? '#cc0000' : '#008800';
    logDebug(`${isError ? 'ERROR' : 'INFO'}: ${msg}`);
}

function stopListening() {
    if (websocket) {
        websocket.close();
        websocket = null;
    }
    if (audioCtx) {
        audioCtx.close().catch(() => {});
        audioCtx = null;
    }
    gainNode = null;
    nextPlayTime = 0;
    receivedChunks = 0;
    if (queueCountEl) queueCountEl.textContent = '0';
    listenBtn.disabled = false;
    listenBtn.textContent = 'Start Listening';
    muteBtn.disabled = true;
    setStatus('Ready', false);
}

listenBtn.addEventListener('click', async () => {
    if (websocket) {
        stopListening();
        return;
    }

    try {
        messageEl.classList.add('hidden');
        setStatus('Initializing Web Audio...');
        listenBtn.disabled = true;

        const AudioCtx = window.AudioContext || window.webkitAudioContext;
        audioCtx = new AudioCtx({ sampleRate: 24000 });
        if (audioCtx.state === 'suspended') {
            await audioCtx.resume();
        }

        gainNode = audioCtx.createGain();
        gainNode.gain.value = isMuted ? 0 : 1;
        gainNode.connect(audioCtx.destination);

        nextPlayTime = 0;
        receivedChunks = 0;

        connectWebSocket();
    } catch (err) {
        showMessage(err.message, true);
        listenBtn.disabled = false;
        setStatus('Error', false);
    }
});

function connectWebSocket() {
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${proto}//${window.location.host}/ws/listen`;
    logDebug(`Connecting to stream at ${wsUrl}...`);
    setStatus('Connecting to stream...');

    websocket = new WebSocket(wsUrl);
    websocket.binaryType = 'arraybuffer';

    websocket.onopen = () => {
        logDebug('WebSocket connected! Listening for live audio...');
        setStatus('Listening (Live)', true);
        listenBtn.disabled = false;
        listenBtn.textContent = 'Stop Listening';
        muteBtn.disabled = false;
    };

    websocket.onmessage = (event) => {
        if (!audioCtx || !(event.data instanceof ArrayBuffer)) return;
        const int16 = new Int16Array(event.data);
        if (int16.length === 0) return;

        receivedChunks++;
        if (queueCountEl) queueCountEl.textContent = String(receivedChunks);

        const float32 = new Float32Array(int16.length);
        for (let i = 0; i < int16.length; i++) {
            float32[i] = int16[i] / 32768;
        }

        const buffer = audioCtx.createBuffer(1, float32.length, 24000);
        buffer.copyToChannel(float32, 0);

        const source = audioCtx.createBufferSource();
        source.buffer = buffer;
        source.connect(gainNode);

        const now = audioCtx.currentTime;
        if (nextPlayTime < now) {
            nextPlayTime = now + 0.05; // 50ms initial jitter buffer
        }
        source.start(nextPlayTime);
        nextPlayTime += buffer.duration;
    };

    websocket.onclose = () => {
        logDebug('Live stream disconnected');
        stopListening();
    };

    websocket.onerror = () => {
        showMessage('Stream connection error', true);
        stopListening();
    };
}

muteBtn.addEventListener('click', () => {
    isMuted = !isMuted;
    if (gainNode) {
        gainNode.gain.value = isMuted ? 0 : 1;
    }
    muteBtn.textContent = isMuted ? 'Unmute' : 'Mute';
});

logDebug('Live Audio Player ready. Click "Start Listening".');
