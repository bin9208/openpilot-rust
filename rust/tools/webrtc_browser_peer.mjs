import { mkdtemp, rm } from 'node:fs/promises';
import { join, dirname } from 'node:path';

const { loadOmowright } = await import(process.env.WEBRTC_OMOWRIGHT_ADAPTER);

const control = process.argv[2];
const profile = await mkdtemp(join(dirname(process.argv[3]), 'browser-profile-'));
const { omowright } = await loadOmowright();
let browser;
try {
  browser = await omowright.connectPipe({ browserPath: process.env.WEBRTC_CHROME || '/usr/bin/google-chrome',
    browserArgs: ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-component-update', '--disable-sync', '--disable-domain-reliability', '--metrics-recording-only', '--disable-dev-shm-usage', `--user-data-dir=${profile}`], storageRoot: profile });
  const page = await browser.newTab('about:blank');
  await page.goto(control, { waitUntil: 'load' });
  const result = await page.evaluate(async ({ browserPid, nodePid, carrot }) => {
    const state = { events: [], messages: [], frames: [], tracks: [], errors: [], sync: [] };
    const peer = new RTCPeerConnection({ iceServers: [] });
    window.peer = peer;
    const channel = peer.createDataChannel('data', { ordered: true });
    channel.binaryType = 'arraybuffer';
    channel.onmessage = event => {
      if (typeof event.data === 'string') state.messages.push(event.data);
      else {
        const bytes = new Uint8Array(event.data);
        state.messages.push(Array.from(bytes));
        if (bytes.length === 12 && String.fromCharCode(...bytes.slice(0, 4)) === 'CVF1') {
          const view = new DataView(event.data);
          state.sync.push({ frameId: view.getUint32(4), rtpTimestamp: view.getUint32(8) });
        }
      }
    };
    peer.onconnectionstatechange = () => state.events.push(peer.connectionState);
    peer.addTransceiver('video', { direction: 'recvonly' });
    const video = document.querySelector('video');
    const canvas = document.createElement('canvas');
    let capture = Promise.resolve();
    video.requestVideoFrameCallback(function observed(now, metadata) {
      capture = capture.then(async () => {
        canvas.width = video.videoWidth; canvas.height = video.videoHeight;
        canvas.getContext('2d').drawImage(video, 0, 0);
        const rgba = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
        const digest = await crypto.subtle.digest('SHA-256', rgba);
        state.frames.push({ width: canvas.width, height: canvas.height, hash: Array.from(new Uint8Array(digest)).map(v => v.toString(16).padStart(2, '0')).join(''), rtpTimestamp: metadata.rtpTimestamp });
      });
      video.requestVideoFrameCallback(observed);
    });
    peer.ontrack = event => {
      state.tracks.push(event.track.id);
      video.srcObject = new MediaStream([event.track]);
      video.muted = true;
      video.play().catch(error => state.errors.push(String(error)));
    };
    try {
      await peer.setLocalDescription(await peer.createOffer());
      if (peer.iceGatheringState !== 'complete') await new Promise(resolve => peer.addEventListener('icegatheringstatechange', () => { if (peer.iceGatheringState === 'complete') resolve(); }));
      const prepared = await fetch('/prepare', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ sdp: peer.localDescription.sdp, browserPid, nodePid }) });
      const answer = await prepared.json();
      state.answerStatus = prepared.status;
      await peer.setRemoteDescription(answer);
      const wait = async predicate => { const deadline = performance.now() + 5000; while (!predicate()) { if (performance.now() >= deadline) throw Error('owned browser wait expired'); await new Promise(resolve => setTimeout(resolve, 5)); } };
      await wait(() => peer.connectionState === 'connected' && channel.readyState === 'open');
      state.connected = peer.connectionState;
      await fetch('/produce', { method: 'POST' });
      await wait(() => state.frames.length >= 3 && state.messages.some(value => value === '{"owned_browser": 240}') && (!carrot || state.sync.length >= 3));
      await capture;
      state.stats = Array.from((await peer.getStats()).values()).filter(value => ['inbound-rtp', 'codec', 'candidate-pair', 'transport'].includes(value.type));
      await fetch('/shutdown', { method: 'POST' });
      await new Promise(resolve => setTimeout(resolve, 300));
      state.preCallerClose = { connection: peer.connectionState, dtls: peer.getReceivers()[0].transport.state, track: peer.getReceivers()[0].track.readyState, channel: channel.readyState };
      return state;
    } finally { peer.close(); }
  }, { browserPid: browser.browserProcess.pid, nodePid: process.pid, carrot: process.argv[4] === '1' });
  console.log(JSON.stringify(result));
  await page.screenshot().then(bytes => import('node:fs/promises').then(fs => fs.writeFile(`${process.argv[3]}/browser.png`, bytes)));
} finally {
  if (browser) await browser.close();
  await rm(profile, { recursive: true, force: true });
}
