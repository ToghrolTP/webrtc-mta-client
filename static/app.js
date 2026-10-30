const startBtn = document.getElementById('startBtn');
const stopBtn = document.getElementById('stopBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const meterFill = document.getElementById('meterFill');
const timerEl = document.getElementById('timer');
const messageEl = document.getElementById('message');

let localStream = null;
let peerConnection = null;
let audioContext = null;
let analyser = null;
let animFrameId = null;
let timerInterval = null;
let startTime = null;

function setStatus(text, isActive = false) {
    statusText.textContent = text;
    statusDot.classList.toggle('active', isActive);
}

function showMessage(msg, isError = false) {
    messageEl.textContent = msg;
    messageEl.classList.remove('hidden');
    messageEl.style.color = isError ? '#ef4444' : '#10b981';
}

function startTimer() {
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

        localStream = await navigator.mediaDevices.getUserMedia({
            audio: { echoCancellation: true, noiseSuppression: true }
        });

        setStatus('Connecting WebRTC...');
        peerConnection = new RTCPeerConnection();
        localStream.getTracks().forEach(track => peerConnection.addTrack(track, localStream));

        startMeter(localStream);

        peerConnection.onconnectionstatechange = () => {
            if (peerConnection.connectionState === 'connected') {
                setStatus('Recording...', true);
                stopBtn.disabled = false;
                startTimer();
            } else if (peerConnection.connectionState === 'disconnected' || peerConnection.connectionState === 'closed') {
                stopRecording();
            }
        };

        const offer = await peerConnection.createOffer();
        await peerConnection.setLocalDescription(offer);

        await new Promise(resolve => {
            if (peerConnection.iceGatheringState === 'complete') {
                resolve();
                return;
            }

            let timeoutId = null;
            const checkIce = () => {
                if (peerConnection.iceGatheringState === 'complete') {
                    peerConnection.removeEventListener('icegatheringstatechange', checkIce);
                    if (timeoutId) clearTimeout(timeoutId);
                    resolve();
                }
            };

            peerConnection.addEventListener('icegatheringstatechange', checkIce);

            timeoutId = setTimeout(() => {
                peerConnection.removeEventListener('icegatheringstatechange', checkIce);
                resolve();
            }, 3000);
        });

        const res = await fetch('/api/offer', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                sdp: peerConnection.localDescription.sdp,
                type: peerConnection.localDescription.type
            })
        });

        if (!res.ok) {
            throw new Error(`Signaling failed: ${res.statusText}`);
        }

        const answer = await res.json();
        await peerConnection.setRemoteDescription(new RTCSessionDescription(answer));

    } catch (err) {
        showMessage(err.message, true);
        stopRecording(null);
    }
});

function stopRecording(statusMessage = 'Recording stopped and saved on server.') {
    stopTimer();
    stopMeter();
    if (localStream) {
        localStream.getTracks().forEach(t => t.stop());
        localStream = null;
    }
    if (peerConnection) {
        peerConnection.close();
        peerConnection = null;
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
