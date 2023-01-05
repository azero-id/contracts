import { ApiPromise, WsProvider } from "@polkadot/api";

async function main() {
  // Create our API with a default connection to the local node
  const provider = new WsProvider("wss://ws.test.azero.dev");
  const api = await ApiPromise.create({ provider });

  // Subscribe to system events via storage
  api.query.system.events((events) => {
    // Loop through the Vec<EventRecord>
    events.forEach(async ({ event }) => {
      console.log("New Event:", event);
    });
  });
}

main().catch((error) => {
  console.error(error);
  process.exit(-1);
});
