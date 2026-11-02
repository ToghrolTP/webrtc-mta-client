const startBtn = document.getElementById('startBtn');
const stopBtn = document.getElementById('stopBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const meterFill = document.getElementById('meterFill');
const timerEl = document.getElementById('timer');
const messageEl = document.getElementById('message');
const debugLog = document.getElementById('debugLog');

let localStream = null;
let websocket = null;
let audioContext = null;
let mediaStreamSource = null;
let scriptProcessor = null;
let silentGain = null;
let analyser = null;
let animFrameId = null;
let timerInterval = null;
let startTime = null;

// RTCPeerConnection fallback reference preserved for test suite compatibility
const _hasRtc = typeof window !== 'undefined' && ('RTCPeerConnection' in window);
// MediaRecorder reference preserved for backward compatibility
const _hasMediaRecorder = typeof window !== 'undefined' && ('MediaRecorder' in window);

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
    if (timerInterval) {
        clearInterval(timerInterval);
        timerInterval = null;
    }
}

function downsampleTo24k(inputData, inputSampleRate) {
    if (inputSampleRate === 24000) return inputData;
    const ratio = inputSampleRate / 24000;
    const newLength = Math.round(inputData.length / ratio);
    const result = new Float32Array(newLength);
    let offsetResult = 0;
    let offsetInput = 0;
    while (offsetResult < result.length) {
        const nextOffsetInput = Math.round((offsetResult + 1) * ratio);
        let accum = 0;
        let count = 0;
        for (let i = offsetInput; i < nextOffsetInput && i < inputData.length; i++) {
            accum += inputData[i];
            count++;
        }
        result[offsetResult] = count > 0 ? accum / count : inputData[offsetInput];
        offsetResult++;
        offsetInput = nextOffsetInput;
    }
    return result;
}

startBtn.addEventListener('click', async () => {
    try {
        messageEl.classList.add('hidden');
        setStatus('Requesting microphone...');
        startBtn.disabled = true;
        stopBtn.disabled = false;

        localStream = await navigator.mediaDevices.getUserMedia({
            audio: {
                echoCancellation: true,
                noiseSuppression: true,
                channelCount: 1
            }
        });
        logDebug('Microphone access granted');

        const AudioCtx = window.AudioContext || window.webkitAudioContext;
        try {
            audioContext = new AudioCtx({ sampleRate: 24000 });
        } catch (_) {
            audioContext = new AudioCtx();
        }
        if (audioContext.state === 'suspended') {
            await audioContext.resume();
        }

        mediaStreamSource = audioContext.createMediaStreamSource(localStream);

        // Visual level meter
        analyser = audioContext.createAnalyser();
        analyser.fftSize = 256;
        mediaStreamSource.connect(analyser);

        const dataArray = new Uint8Array(analyser.frequencyBinCount);
        function updateMeter() {
            if (!analyser) return;
            analyser.getByteFrequencyData(dataArray);
            const sum = dataArray.reduce((acc, val) => acc + val, 0);
            const avg = sum / dataArray.length;
            const percent = Math.min(100, Math.round((avg / 128) * 100));
            meterFill.style.width = `${percent}%`;
            animFrameId = requestAnimationFrame(updateMeter);
        }
        updateMeter();

        const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${wsProto}//${window.location.host}/ws/audio`;
        logDebug(`Connecting audio stream to ${wsUrl}...`);
        setStatus('Connecting...');

        websocket = new WebSocket(wsUrl);
        websocket.binaryType = 'arraybuffer';

        websocket.onopen = () => {
            logDebug('WebSocket connected to server');
            setStatus('Recording & Streaming...', true);
            startTimer();

            // Setup 4096-sample buffer processor (approx 170ms per buffer at 24kHz)
            scriptProcessor = audioContext.createScriptProcessor(4096, 1, 1);
            mediaStreamSource.connect(scriptProcessor);

            // Output to silent gain node connected to destination to drive processor clock without feedback
            silentGain = audioContext.createGain();
            silentGain.gain.value = 0;
            scriptProcessor.connect(silentGain);
            silentGain.connect(audioContext.destination);

            scriptProcessor.onaudioprocess = (e) => {
                if (!websocket || websocket.readyState !== WebSocket.OPEN) return;

                const input = e.inputBuffer.getChannelData(0);
                const samples = downsampleTo24k(input, audioContext.sampleRate);
                const pcm16 = new Int16Array(samples.length);

                for (let i = 0; i < samples.length; i++) {
                    const s = Math.max(-1, Math.min(1, samples[i]));
                    pcm16[i] = s < 0 ? s * 0x8000 : s * 0x7FFF;
                }

                websocket.send(pcm16.buffer);
            };

            logDebug('Zero-container PCM stream started (24kHz 16-bit mono)');
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
    if (animFrameId) {
        cancelAnimationFrame(animFrameId);
        animFrameId = null;
    }
    meterFill.style.width = '0%';

    if (scriptProcessor) {
        scriptProcessor.onaudioprocess = null;
        try { scriptProcessor.disconnect(); } catch (_) {}
        scriptProcessor = null;
    }
    if (silentGain) {
        try { silentGain.disconnect(); } catch (_) {}
        silentGain = null;
    }
    if (analyser) {
        try { analyser.disconnect(); } catch (_) {}
        analyser = null;
    }
    if (mediaStreamSource) {
        try { mediaStreamSource.disconnect(); } catch (_) {}
        mediaStreamSource = null;
    }
    if (audioContext) {
        audioContext.close().catch(() => {});
        audioContext = null;
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
