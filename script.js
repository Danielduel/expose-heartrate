const ws = new WebSocket("ws://localhost:2137", "rust-websocket");
let connected = false;
let heartrate = 0;

ws.onmessage = (msg) => {
  if (connected) {
    heartrate = msg.data;
  }
  connected = true;
  // console.log(msg.data);
}

window.onload = () => {
  const reading = document.getElementById("reading");
  reading.innerText = 0;

  setInterval(() => {
    if (connected) {
      ws.send("bpm");
      reading.innerText = heartrate;
    }
  }, 1000);
}
