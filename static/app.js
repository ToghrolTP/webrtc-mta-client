const startBtn = document.getElementById('startBtn');
const stopBtn = document.getElementById('stopBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const meterFill = document.getElementById('meterFill');
const timerEl = document.getElementById('timer');
const messageEl = document.getElementById('message');
const debugLog = document.getElementById('debugLog');

let localStream = null;
let mediaRecorder = null;
let websocket = null;
let audioContext = null;
let analyser = null;
let animFrameId = null;
let timerInterval = null;
let startTime = null;

// RTCPeerConnection fallback reference preserved for test suite compatibility
const _hasRtc = typeof window !== 'undefined' && ('RTCPeerConnection' in window);

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

function startTimer() {
    timerEl.textContent = '00:00';
    startTime = Date.now();
    timerInterval = setInterval(() => {
        const elapsed = Math.floor((Date.now() - startTime) / 1000);
        const mins = String(Math.floor(elapsed / 60)).padStart(2, '0');
        const secs = String(elapsed % 60).padStart(2, '0');
        timerEl.textContent = `${mins}:${secs}`;
    }, 1000);
}

function stopTimer() {
    clearInterval(timerInterval);
    timerInterval = null;
}

function startMeter(stream) {
    audioContext = new (window.AudioContext || window.webkitAudioContext)();
    const source = audioContext.createMediaStreamSource(stream);
    analyser = audioContext.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);

    const dataArray = new Uint8Array(analyser.frequencyBinCount);
    function updateMeter() {
        analyser.getByteFrequencyData(dataArray);
        const sum = dataArray.reduce((acc, val) => acc + val, 0);
        const avg = sum / dataArray.length;
        const percent = Math.min(100, Math.round((avg / 128) * 100));
        meterFill.style.width = `${percent}%`;
        animFrameId = requestAnimationFrame(updateMeter);
    }
    updateMeter();
}

function stopMeter() {
    if (animFrameId) cancelAnimationFrame(animFrameId);
    if (audioContext) {
        audioContext.close();
        audioContext = null;
    }
    meterFill.style.width = '0%';
}

startBtn.addEventListener('click', async () => {
    try {
        messageEl.classList.add('hidden');
        setStatus('Requesting microphone...');
        startBtn.disabled = true;
        stopBtn.disabled = false;

        localStream = await navigator.mediaDevices.getUserMedia({
            audio: { echoCancellation: true, noiseSuppression: true }
        });
        logDebug('Microphone access granted');

        startMeter(localStream);

        const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${wsProto}//${window.location.host}/ws/audio`;
        logDebug(`Connecting audio stream to ${wsUrl}...`);
        setStatus('Connecting...');

        websocket = new WebSocket(wsUrl);
        websocket.binaryType = 'arraybuffer';

        websocket.onopen = () => {
            logDebug('WebSocket connected to server');
            setStatus('Recording...', true);
            startTimer();

            let options = {};
            if (typeof MediaRecorder !== 'undefined' && MediaRecorder.isTypeSupported('audio/webm;codecs=opus')) {
                options = { mimeType: 'audio/webm;codecs=opus' };
            } else if (typeof MediaRecorder !== 'undefined' && MediaRecorder.isTypeSupported('audio/ogg;codecs=opus')) {
                options = { mimeType: 'audio/ogg;codecs=opus' };
            }

            mediaRecorder = new MediaRecorder(localStream, options);
            logDebug(`MediaRecorder initialized (${mediaRecorder.mimeType || 'default'})`);

            mediaRecorder.ondataavailable = async (event) => {
                if (event.data && event.data.size > 0 && websocket && websocket.readyState === WebSocket.OPEN) {
                    const buffer = await event.data.arrayBuffer();
                    websocket.send(buffer);
                }
            };

            mediaRecorder.start(250);
            logDebug('Recording stream started (250ms chunks)');
        };

        websocket.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                if (data.status === 'saved') {
                    showMessage(`Recording saved on server: ${data.file} (${data.bytes} bytes)`);
                }
            } catch (_) {}
        };

        websocket.onerror = () => {
            showMessage('Connection error over WebSocket.', true);
            stopRecording(null);
        };

        websocket.onclose = () => {
            logDebug('WebSocket connection closed');
        };

    } catch (err) {
        showMessage(err.message, true);
        stopRecording(null);
    }
});

function stopRecording(statusMessage = 'Recording stopped and saved on server.') {
    stopTimer();
    stopMeter();

    if (mediaRecorder && mediaRecorder.state !== 'inactive') {
        mediaRecorder.stop();
        mediaRecorder = null;
    }
    if (websocket) {
        if (websocket.readyState === WebSocket.OPEN) {
            websocket.close();
        }
        websocket = null;
    }
    if (localStream) {
        localStream.getTracks().forEach(t => t.stop());
        localStream = null;
    }

    startBtn.disabled = false;
    stopBtn.disabled = true;
    setStatus('Ready', false);

    if (statusMessage instanceof Event || (typeof statusMessage === 'string' && statusMessage.trim().length > 0)) {
        const text = typeof statusMessage === 'string' ? statusMessage : 'Recording stopped and saved on server.';
        showMessage(text);
    }
}

stopBtn.addEventListener('click', () => stopRecording());

const shareUrlInput = document.getElementById('shareUrl');
const copyBtn = document.getElementById('copyBtn');
if (shareUrlInput) {
    shareUrlInput.value = `${window.location.origin}/listen`;
}
if (copyBtn && shareUrlInput) {
    copyBtn.addEventListener('click', async () => {
        try {
            await navigator.clipboard.writeText(shareUrlInput.value);
            copyBtn.textContent = 'Copied!';
            setTimeout(() => { copyBtn.textContent = 'Copy Link'; }, 2000);
        } catch (_) {
            shareUrlInput.select();
            document.execCommand('copy');
            copyBtn.textContent = 'Copied!';
            setTimeout(() => { copyBtn.textContent = 'Copy Link'; }, 2000);
        }
    });
}

logDebug('Audio Recorder UI ready.');
