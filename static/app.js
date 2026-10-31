const startBtn = document.getElementById('startBtn');
const stopBtn = document.getElementById('stopBtn');
const statusDot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const meterFill = document.getElementById('meterFill');
const timerEl = document.getElementById('timer');
const messageEl = document.getElementById('message');
const debugLog = document.getElementById('debugLog');

let localStream = null;
let peerConnection = null;
let audioContext = null;
let analyser = null;
let animFrameId = null;
let timerInterval = null;
let startTime = null;
let connectionTimeout = null;

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

        setStatus('Connecting WebRTC...');
        peerConnection = new RTCPeerConnection({
            iceServers: [
                { urls: 'stun:stun.l.google.com:19302' },
                { urls: 'stun:stun1.l.google.com:19302' },
                { urls: 'stun:openrelay.metered.ca:80' },
                {
                    urls: [
                        'turn:openrelay.metered.ca:80',
                        'turn:openrelay.metered.ca:443',
                        'turn:openrelay.metered.ca:443?transport=tcp'
                    ],
                    username: 'openrelayproject',
                    credential: 'openrelayproject'
                }
            ]
        });

        peerConnection.onicecandidate = (event) => {
            if (event.candidate) {
                logDebug(`Local candidate: ${event.candidate.type} ${event.candidate.protocol} ${event.candidate.address || ''}:${event.candidate.port || ''}`);
            } else {
                logDebug('Local ICE gathering complete');
            }
        };

        peerConnection.onicecandidateerror = (event) => {
            logDebug(`ICE candidate error: ${event.errorText || 'unknown'} (${event.errorCode || 0})`);
        };

        localStream.getTracks().forEach(track => {
            logDebug(`Adding audio track: ${track.id || track.label}`);
            peerConnection.addTrack(track, localStream);
        });

        startMeter(localStream);

        connectionTimeout = setTimeout(() => {
            if (peerConnection && peerConnection.connectionState !== 'connected') {
                showMessage('WebRTC connection timed out (NAT/firewall blocked media relay).', true);
                stopRecording(null);
            }
        }, 15000);

        peerConnection.onconnectionstatechange = () => {
            const state = peerConnection.connectionState;
            logDebug(`WebRTC connection state -> ${state}`);
            if (state === 'connected') {
                if (connectionTimeout) {
                    clearTimeout(connectionTimeout);
                    connectionTimeout = null;
                }
                setStatus('Recording...', true);
                stopBtn.disabled = false;
                startBtn.disabled = true;
                startTimer();
            } else if (state === 'failed') {
                showMessage('WebRTC connection failed (firewall or network error).', true);
                stopRecording(null);
            } else if (state === 'disconnected' || state === 'closed') {
                stopRecording();
            }
        };

        peerConnection.oniceconnectionstatechange = () => {
            logDebug(`ICE connection state -> ${peerConnection.iceConnectionState}`);
        };

        const offer = await peerConnection.createOffer();
        await peerConnection.setLocalDescription(offer);
        logDebug('Created local SDP offer');

        await new Promise(resolve => {
            if (peerConnection.iceGatheringState === 'complete') {
                resolve();
                return;
            }

            let timeoutId = null;
            const checkIce = () => {
                logDebug(`ICE gathering state -> ${peerConnection.iceGatheringState}`);
                if (peerConnection.iceGatheringState === 'complete') {
                    peerConnection.removeEventListener('icegatheringstatechange', checkIce);
                    if (timeoutId) clearTimeout(timeoutId);
                    resolve();
                }
            };

            peerConnection.addEventListener('icegatheringstatechange', checkIce);

            timeoutId = setTimeout(() => {
                peerConnection.removeEventListener('icegatheringstatechange', checkIce);
                logDebug('ICE gathering timeout reached (proceeding with gathered candidates)');
                resolve();
            }, 3000);
        });

        logDebug('Sending SDP offer to /api/offer');
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
        logDebug('Received SDP answer from server');
        await peerConnection.setRemoteDescription(new RTCSessionDescription(answer));

    } catch (err) {
        showMessage(err.message, true);
        stopRecording(null);
    }
});

function stopRecording(statusMessage = 'Recording stopped and saved on server.') {
    if (connectionTimeout) {
        clearTimeout(connectionTimeout);
        connectionTimeout = null;
    }
    stopTimer();
    stopMeter();
    if (localStream) {
        localStream.getTracks().forEach(t => t.stop());
        localStream = null;
    }
    if (peerConnection) {
        peerConnection.onconnectionstatechange = null;
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

logDebug('Debug UI initialized. Ready.');
