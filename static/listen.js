const listenBtn = document.getElementById('listenBtn');
const muteBtn = document.getElementById('muteBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const queueCountEl = document.getElementById('queueCount');
const audioEl = document.getElementById('audioEl');
const debugLog = document.getElementById('debugLog');
const messageEl = document.getElementById('message');

let websocket = null;
let mediaSource = null;
let sourceBuffer = null;
let queue = [];

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
    if (mediaSource && mediaSource.readyState === 'open') {
        try {
            mediaSource.endOfStream();
        } catch (_) {}
    }
    mediaSource = null;
    sourceBuffer = null;
    queue = [];
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
        setStatus('Initializing media player...');
        listenBtn.disabled = true;

        mediaSource = new MediaSource();
        audioEl.src = URL.createObjectURL(mediaSource);

        mediaSource.addEventListener('sourceopen', () => {
            logDebug('MediaSource ready');
            const mime = MediaSource.isTypeSupported('audio/webm; codecs="opus"')
                ? 'audio/webm; codecs="opus"'
                : (MediaSource.isTypeSupported('audio/ogg; codecs="opus"') ? 'audio/ogg; codecs="opus"' : 'audio/webm');

            logDebug(`SourceBuffer format: ${mime}`);
            sourceBuffer = mediaSource.addSourceBuffer(mime);
            sourceBuffer.mode = 'sequence';

            sourceBuffer.addEventListener('updateend', () => {
                if (queue.length > 0 && !sourceBuffer.updating) {
                    const next = queue.shift();
                    if (queueCountEl) queueCountEl.textContent = String(queue.length);
                    sourceBuffer.appendBuffer(next);
                }
            });

            connectWebSocket();
        });

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
        logDebug('WebSocket connected! Listening for incoming audio...');
        setStatus('Listening (Live)', true);
        listenBtn.disabled = false;
        listenBtn.textContent = 'Stop Listening';
        muteBtn.disabled = false;
        audioEl.play().catch(e => logDebug(`Audio playback notice: ${e.message}`));
    };

    websocket.onmessage = (event) => {
        const chunk = new Uint8Array(event.data);
        if (sourceBuffer) {
            if (sourceBuffer.updating || queue.length > 0) {
                queue.push(chunk);
                if (queueCountEl) queueCountEl.textContent = String(queue.length);
            } else {
                try {
                    sourceBuffer.appendBuffer(chunk);
                } catch (e) {
                    logDebug(`Buffer error: ${e.message}`);
                }
            }
        }
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
    audioEl.muted = !audioEl.muted;
    muteBtn.textContent = audioEl.muted ? 'Unmute' : 'Mute';
});

logDebug('Live Audio Player ready. Click "Start Listening".');
